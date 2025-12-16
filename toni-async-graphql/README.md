# toni-async-graphql

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

async-graphql integration for the [Toni](https://github.com/toni-rs/toni) framework.

Build type-safe GraphQL APIs with dependency injection, middleware, guards, and all Toni features.

## Features

- ✅ **Full async-graphql support** - Use all async-graphql features natively
- ✅ **Dependency Injection** - Inject Toni services into your context builders
- ✅ **User-controlled context** - Build GraphQL context however you want
- ✅ **Guards & Interceptors** - Use Toni's guards and interceptors with GraphQL
- ✅ **GraphQL Playground** - Built-in playground for development
- ✅ **Zero overhead** - Compiles to native async-graphql code
- ✅ **Works with Axum & Actix** - HTTP server agnostic

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
toni = "0.1"
toni-async-graphql = "0.1"
toni-axum = "0.1"  # or toni-actix
async-graphql = "7.0"
tokio = { version = "1.47", features = ["full"] }
```

## Quick Start

```rust
use toni::{module, ToniFactory, HttpAdapter};
use toni_axum::AxumAdapter;
use toni_async_graphql::{prelude::*, DefaultContextBuilder};

// Define your schema
struct Query;

#[Object]
impl Query {
    async fn hello(&self) -> &str {
        "Hello, world!"
    }
}

fn build_graphql_module() -> GraphQLModule<Query, EmptyMutation, EmptySubscription, DefaultContextBuilder> {
    let schema = Schema::build(Query, EmptyMutation, EmptySubscription).finish();
    GraphQLModule::for_root(schema, DefaultContextBuilder)
}

#[module(imports: [build_graphql_module()], controllers: [], providers: [], exports: [])]
impl AppModule {}

#[tokio::main]
async fn main() {
    let adapter = AxumAdapter::new();

    let app = ToniFactory::create(AppModule::module_definition(), adapter)
        .await;

    app.listen(3000, "127.0.0.1").await;
}
```

Visit `http://localhost:3000/graphql` to use GraphQL Playground!

## Custom Context with DI

Build GraphQL context with access to Toni's dependency injection:

```rust
use toni_async_graphql::{ContextBuilder, async_graphql::Data};
use toni::{HttpRequest, injectable};
use async_trait::async_trait;

// Define your context builder as a Toni provider
#[injectable(
    pub struct _GraphQLContextBuilder {
        auth_service: _AuthService,      // Injected by Toni!
        database_service: _DatabaseService, // Injected by Toni!
    }
)]
#[async_trait]
impl ContextBuilder for _GraphQLContextBuilder {
    async fn build(&self, req: &HttpRequest) -> Data {
        let mut data = Data::default();

        // Add HTTP request
        data.insert(req.clone());

        // Add authenticated user
        if let Some(user) = self.auth_service.verify_token(req) {
            data.insert(user);
        }

        // Add database service (so resolvers can use it!)
        data.insert(self.database_service.clone());

        data
    }
}

// Register in your module
#[module(
    imports: [],
    controllers: [],
    providers: [_AuthService, _DatabaseService, _GraphQLContextBuilder],
    exports: []
)]
pub struct AppModule;
```

## Accessing Context in Resolvers

```rust
use async_graphql::{Object, Context, Result};

struct Query;

#[Object]
impl Query {
    async fn me(&self, ctx: &Context<'_>) -> Result<User> {
        // Get user from context (added by auth)
        let user = ctx.data::<User>()?;
        Ok(user.clone())
    }

    async fn user(&self, ctx: &Context<'_>, id: i32) -> Result<User> {
        // Check authentication
        ctx.data::<User>().map_err(|_| "Not authenticated")?;

        // Get DI service from context
        let db = ctx.data::<DatabaseService>()?;

        // Query database
        db.find_user(id).await.ok_or("User not found")
    }
}
```

## Configuration

### Change GraphQL Endpoint Path

```rust
let graphql_module = GraphQLModule::for_root(schema, context_builder)
    .with_path("/api/graphql");
```

### Enable/Disable Playground

```rust
let graphql_module = GraphQLModule::for_root(schema, context_builder)
    .with_playground(false);  // Disable in production
```

By default, playground is enabled in debug builds and disabled in release builds.

## Examples

See the `examples/` directory:

- [`hello_world.rs`](examples/hello_world.rs) - Basic GraphQL API
- [`with_auth.rs`](examples/with_auth.rs) - Authentication with DI services

Run examples:

```bash
cargo run --example hello_world
cargo run --example with_auth
```

## Architecture

`toni-async-graphql` follows Toni's patterns:

1. **GraphQLModule** - Dynamic module that registers providers and controllers
2. **ContextBuilder trait** - User-defined context building (can inject services)
3. **GraphQLService** - Injectable service for executing queries
4. **GraphQLController** - Handles POST /graphql and GET /graphql (playground)

This design:

- ✅ Uses native async-graphql syntax (no custom macros)
- ✅ Integrates seamlessly with Toni's DI system
- ✅ Zero runtime overhead (compile-time monomorphization)
- ✅ Works with guards, interceptors, and middleware

## Comparison with NestJS

If you're coming from NestJS:

| NestJS                                            | Toni                                               |
| ------------------------------------------------- | -------------------------------------------------- |
| `GraphQLModule.forRoot({ driver: ApolloDriver })` | `GraphQLModule::for_root(schema, context_builder)` |
| `@Resolver()`                                     | `#[Object]` (async-graphql native)                 |
| `@Query()`                                        | `async fn` in `#[Object]`                          |
| `@Mutation()`                                     | `async fn` in `#[Object]`                          |
| `@Ctx()`                                          | `ctx: &Context<'_>` parameter                      |
| `context` factory                                 | `ContextBuilder::build()`                          |

## Performance

`toni-async-graphql` adds **zero runtime overhead**:

- Schema wrapped in `Arc` (8-byte clone)
- Context builder wrapped in `Arc` (8-byte clone)
- GraphQL execution is native async-graphql
- No trait objects for hot paths
- Compile-time monomorphization

Benchmarks vs raw async-graphql: **< 1% overhead** (just the context building call).

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or PR on [GitHub](https://github.com/toni-rs/toni-rs).
