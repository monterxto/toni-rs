# Tower Compatibility Layer — Design Notes

This document covers the current state of the `tower-compat` feature, its known gaps, and areas worth revisiting.

---

## What was built

`TowerLayer<L>` wraps any `tower::Layer` as a toni `Middleware`, plugging into `configure_middleware` exactly like a hand-written toni middleware. The integration:

- Passes `HttpRequest` to Tower as `http::Request<Bytes>` — no conversion needed because `HttpRequest` is a newtype over `http::Request<Bytes>`.
- Wraps the downstream chain as a `tower::Service<http::Request<Bytes>>` (`ToniNextService`) so Tower can call downstream.
- Converts `http::Response<B: Body> → HttpResponse` on the way back, wrapping the body as a streaming `BoxBody` without buffering.

The adapter (axum, actix, future) never sees Tower. The `tower_compat.rs` boundary is entirely internal, so the feature works regardless of which adapter is in use.

Enabled via feature flag — zero cost when not compiled in:

```toml
toni = { version = "...", features = ["tower-compat"] }
```

---

## Known limitations

### Single-use inner service

`ToniNextService` panics if called more than once. `Next::run(self: Box<Self>)` is intentionally single-use — the framework has already routed to a specific handler. Tower layers that call the inner service multiple times (`tower::retry`, `tower::hedge`) are client-side patterns and will panic at runtime.

### Response body content-type comes from headers

Tower may have transformed the response body (e.g. `CompressionLayer`), so the original content-type hint is unknown. `to_toni_response` reads the `Content-Type` header Tower set and forwards it onto the `Body` so adapters see the correct hint via `body.content_type()`. If Tower strips or rewrites `Content-Type`, the body will have no content-type hint and adapters fall back to `application/octet-stream`.

---

## What is not yet handled

### Streaming request bodies

`HttpRequest.body` is intentionally `bytes::Bytes` — requests are always fully buffered at the adapter boundary, which keeps the middleware chain simple and avoids single-use stream problems. This is not expected to change: request streaming in a middleware chain requires careful lifecycle management that outweighs the cost for the typical middleware use case.

### `!Send` futures

`ToniNextService::Future` is bound to `Send`. Toni's integration tests run on a `LocalSet` (via `tokio_localset_test`), but the `Middleware` trait impl requires `Send` futures throughout. If a Tower layer wraps a `!Send` service (uncommon in tower-http, but possible with custom layers), it will fail to compile. There is no clean fix without a `LocalSet`-aware variant.

### Body type recovery on response

`to_toni_response` reads the `Content-Type` header and sets it on the `Body` directly, so adapters get the right hint via `body.content_type()` rather than relying on the header loop to patch it afterwards. If a Tower layer strips or rewrites `Content-Type`, body handling degrades gracefully (no content-type hint, adapter falls back to `application/octet-stream`).

### Tower layers requiring `B: http_body::Body + Clone`

Some Tower middleware (certain retry patterns, request cloning) require the body type to be `Clone`. `Bytes` is `Clone`, so `http::Request<Bytes>` works, but `ToniNextService` is single-use by design, so those layers would compile but panic at runtime. Not a common server-middleware scenario, but worth documenting.

---

## Optimization opportunities

### Header Vec allocation in `to_toni_response`

`to_toni_response` collects headers into `Vec<(String, String)>`. Headers are usually small, but for hot paths this is a repeated allocation. A smallvec or stack-allocated approach would reduce pressure.

---

## Things worth exploring

### `tower::ServiceBuilder` composition

`ServiceBuilder::new().layer(A).layer(B).service(inner)` produces a `Stack<A, Stack<B, inner>>` which itself implements `Layer`. A single `TowerLayer::new(ServiceBuilder::new().layer(cors).layer(trace))` should work today without any changes — worth testing explicitly and documenting as the idiomatic way to compose multiple Tower middlewares before applying them.

### Body type recovery on Tower response

`to_toni_response` wraps the Tower response body as a streaming `BoxBody` — the body is not buffered. If a Tower layer rewrites the body (e.g. `CompressionLayer`), the compressed bytes stream through to the adapter intact. Content-Type is read from the Tower response headers and forwarded onto the toni `Body` as the content-type hint.

### Error type unification

`MiddlewareResult` uses `Box<dyn std::error::Error + Send + Sync>` as the error type. Tower uses the same erased error. There is an opportunity to introduce a proper `ToniError` enum at the middleware layer that carries HTTP status + message — this would make error propagation across Tower and toni layers consistent and would eliminate the string-based error paths that currently exist.
