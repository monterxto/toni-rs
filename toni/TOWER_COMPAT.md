# Tower Compatibility Layer — Design Notes

This document covers the current state of the `tower-compat` feature, its known gaps, and areas worth revisiting.

---

## What was built

`TowerLayer<L>` wraps any `tower::Layer` as a toni `Middleware`, plugging into `configure_middleware` exactly like a hand-written toni middleware. The bridge:

- Converts `HttpRequest → http::Request<Bytes>` before handing off to Tower.
- Wraps `Box<dyn Next>` as a `tower::Service<http::Request<Bytes>>` (`ToniNextService`) so Tower can call downstream.
- Converts `http::Response<Bytes> → HttpResponse` on the way back.
- Preserves path params across the round-trip via `ToniPathParams` stored in `http::Request` extensions (Tower has no path-param concept).
- Preserves toni extensions across the round-trip via `ToniExtensionBridge` — the full `HttpRequest.extensions` map is wrapped as a single opaque entry in `http::Request` extensions and restored intact before downstream toni middleware runs.

The adapter (axum, actix, future) never sees Tower. The conversion is entirely internal to `tower_compat.rs`, so the feature works regardless of which adapter is in use.

Enabled via feature flag — zero cost when not compiled in:

```toml
toni = { version = "...", features = ["tower-compat"] }
```

---

## Known limitations

### Tower middleware cannot read toni-typed extensions

Toni extensions set by preceding toni middleware survive the Tower round-trip — they are restored on `HttpRequest` for any downstream toni middleware. However, Tower middleware itself (e.g. `TraceLayer`, a custom `tower::Layer`) cannot read them, because `ToniExtensionBridge` is an internal type.

Tower middleware reads from `http::Request` extensions by type — it calls `req.extensions().get::<SomeType>()`. It will only find types it was explicitly written to look for. `ToniExtensionBridge` is sitting in the map, but no off-the-shelf Tower layer knows to ask for it.

**If a custom Tower layer needs access to toni extension data**, use the `toni_extensions` helper:

```rust
use toni::tower_compat::toni_extensions;

// inside your Tower layer's Service::call:
if let Some(ext) = toni_extensions(&req) {
    if let Some(user) = ext.get::<MyUser>() {
        // use user
    }
}
```

`toni_extensions` reads from the internal `ToniExtensionBridge` entry without exposing the internal type. Off-the-shelf Tower layers (e.g. `TraceLayer`) will never call this — it is only useful in custom layers you control.

### Single-use inner service

`ToniNextService` panics if called more than once. `Next::run(self: Box<Self>)` is intentionally single-use — the framework has already routed to a specific handler. Tower layers that call the inner service multiple times (`tower::retry`, `tower::hedge`) are client-side patterns and will panic at runtime.

### Response body is always `Body::Binary`

`to_toni_response` always wraps the body as `Body::Binary`. Tower may have transformed it (e.g. `CompressionLayer`), so the original type is unknown. Adapters that set `Content-Type` based on `Body` variant will see `Binary` for all Tower-processed responses; the correct `Content-Type` will be whatever Tower (or the controller's own headers) set.

---

## What is not yet handled

### Streaming bodies

Both `to_http_request` and `ToniNextService::call` buffer the full body into `Bytes`. Large upload/download paths (file streaming, chunked responses) will load the entire body into memory. Proper streaming would require `HttpRequest.body` and `HttpResponse.body` to carry an `http_body::Body` impl rather than the current owned `Body` enum. That is a deeper framework change.

### `!Send` futures

`ToniNextService::Future` is bound to `Send`. Toni's integration tests run on a `LocalSet` (via `tokio_localset_test`), but the `Middleware` trait impl requires `Send` futures throughout. If a Tower layer wraps a `!Send` service (uncommon in tower-http, but possible with custom layers), it will fail to compile. There is no clean fix without a `LocalSet`-aware variant.

### Body type recovery on response

Currently the response `Content-Type` header is the only signal for what kind of body came back. The axum adapter reads headers to decide how to respond downstream. If a Tower layer strips or rewrites `Content-Type`, body handling will degrade gracefully (binary fallback), but this is worth testing explicitly with `CompressionLayer` or `BodyTransformLayer`.

### Tower layers requiring `B: http_body::Body + Clone`

Some Tower middleware (certain retry patterns, request cloning) require the body type to be `Clone`. `Bytes` is `Clone`, so `http::Request<Bytes>` works, but `ToniNextService` is single-use by design, so those layers would compile but panic at runtime. Not a common server-middleware scenario, but worth documenting.

---

## Optimization opportunities

### Redundant allocations in body conversion

`Body::Text(s)` → `Bytes::from(s.into_bytes())` → (if Tower doesn't touch body) → `Body::Text(String::from_utf8(...))` allocates twice and copies the string data. If Tower does not transform the body, this round-trip is pure overhead. An optimization would be to detect pass-through (e.g. compare body bytes length pre/post) or to skip conversion for body-transparent layers, though there is no clean way to detect this without Tower-layer cooperation.

### Header Vec allocation

Both `to_http_request` and `to_toni_response` collect headers into `Vec<(String, String)>`. Headers are usually small, but for hot paths this is a repeated allocation. A smallvec or stack-allocated approach would reduce pressure.

### `ToniPathParams` and `ToniExtensionBridge` clone bounds

Both `ToniPathParams` and `ToniExtensionBridge` derive `Clone` because `http::Extensions::insert` requires `T: Clone`. However, both are removed from extensions after the first (and only) use via `remove`, which does not require `Clone`. The bound exists solely to satisfy `insert`. A wrapper type that implements `Clone` trivially (or a different storage mechanism) could avoid the requirement if the clone turns out to be measurable overhead — unlikely given extensions are small.

---

## Things worth exploring

### `tower::ServiceBuilder` composition

`ServiceBuilder::new().layer(A).layer(B).service(inner)` produces a `Stack<A, Stack<B, inner>>` which itself implements `Layer`. A single `TowerLayer::new(ServiceBuilder::new().layer(cors).layer(trace))` should work today without any changes — worth testing explicitly and documenting as the idiomatic way to compose multiple Tower middlewares before applying them.

### Streaming support design

The right path to streaming is making `Body` a trait object rather than an enum. The bridge would then hold `Box<dyn http_body::Body<Data = Bytes, Error = ...>>` and avoid buffering. This is a breaking change to `HttpRequest`/`HttpResponse` and should be evaluated as a separate framework-level concern, not specific to tower-compat.

### Error type unification

`MiddlewareResult` uses `Box<dyn std::error::Error + Send + Sync>` as the error type. Tower uses the same erased error. There is an opportunity to introduce a proper `ToniError` enum at the middleware layer that carries HTTP status + message — this would make error propagation across Tower and toni layers consistent and would eliminate the string-based error paths that currently exist.
