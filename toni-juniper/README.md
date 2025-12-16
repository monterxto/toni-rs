# toni-juniper

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Juniper GraphQL integration for the [Toni](https://github.com/toni-rs/toni) framework.

Build type-safe GraphQL APIs with dependency injection, middleware, guards, and all Toni features.

## Features

- ✅ **Full Juniper support** - Use all Juniper features natively
- ✅ **Dependency Injection** - Inject Toni services into your context builders
- ✅ **User-controlled context** - Build GraphQL context however you want
- ✅ **Guards & Interceptors** - Use Toni's guards and interceptors with GraphQL
- ✅ **GraphQL Playground** - Built-in playground for development
- ✅ **Zero overhead** - Compiles to native Juniper code
- ✅ **Works with Axum & Actix** - HTTP server agnostic

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
toni = "0.1"
toni-juniper = "0.1"
toni-axum = "0.1"  # or toni-actix
juniper = "0.16"
tokio = { version = "1.47", features = ["full"] }
```

## Quick Start

```rust
use juniper::{graphql_object, EmptyMutation, EmptySubscription, RootNode};
use toni::{module, ToniFactory, HttpAdapter};
use toni_axum::AxumAdapter;
use toni_juniper::{GraphQLModule, DefaultContextBuilder, DefaultContext};

// Define your schema
struct Query;

#[graphql_object(context = DefaultContext)]
impl Query {
    fn hello() -> &'static str {
        "Hello, world!"
    }
}

fn build_graphql_module() -> GraphQLModule<Query, EmptyMutation<DefaultContext>, EmptySubscription<DefaultContext>, DefaultContextBuilder> {
    let schema = RootNode::new(Query, EmptyMutation::new(), EmptySubscription::new());
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
use toni_juniper::{ContextBuilder, juniper};
use toni::{HttpRequest, injectable};
use async_trait::async_trait;

// Define your context type
#[derive(Clone)]
struct MyContext {
    user_id: Option<i32>,
    db: DatabaseService,
}

impl juniper::Context for MyContext {}

// Define your context builder as a Toni provider
#[injectable(
    pub struct _MyContextBuilder {
        auth_service: _AuthService,      // Injected by Toni!
        db_service: _DatabaseService,    // Injected by Toni!
    }
)]
#[async_trait]
impl ContextBuilder for _MyContextBuilder {
    type Context = MyContext;

    async fn build(&self, req: &HttpRequest) -> Self::Context {
        MyContext {
            user_id: self.auth_service.verify_token(req),
            db: self.db_service.clone(),
        }
    }
}

// Register in your module
#[module(
    imports: [],
    controllers: [],
    providers: [_AuthService, _DatabaseService, _MyContextBuilder],
    exports: []
)]
pub struct AppModule;
```

## Accessing Context in Resolvers

```rust
use juniper::{graphql_object, FieldResult};

struct Query;

#[graphql_object(context = MyContext)]
impl Query {
    fn me(context: &MyContext) -> FieldResult<User> {
        // Get user from context (added by auth)
        let user_id = context.user_id.ok_or("Not authenticated")?;
        Ok(User { id: user_id })
    }

    fn user(context: &MyContext, id: i32) -> FieldResult<User> {
        // Use DI service from context
        context.db.find_user(id).ok_or("User not found")
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

`toni-juniper` follows Toni's patterns:

1. **GraphQLModule** - Dynamic module that registers providers and controllers
2. **ContextBuilder trait** - User-defined context building (can inject services)
3. **GraphQLService** - Injectable service for executing queries
4. **GraphQLController** - Handles POST /graphql and GET /graphql (playground)

This design:

- ✅ Uses native Juniper syntax (no custom macros)
- ✅ Integrates seamlessly with Toni's DI system
- ✅ Zero runtime overhead (compile-time monomorphization)
- ✅ Works with guards, interceptors, and middleware

## Comparison with NestJS

If you're coming from NestJS:

| NestJS                                            | Toni                                               |
| ------------------------------------------------- | -------------------------------------------------- |
| `GraphQLModule.forRoot({ driver: ApolloDriver })` | `GraphQLModule::for_root(schema, context_builder)` |
| `@Resolver()`                                     | `#[graphql_object]` (Juniper native)               |
| `@Query()`                                        | `fn` in `#[graphql_object]`                        |
| `@Mutation()`                                     | `fn` in `#[graphql_object]`                        |
| `@Context()`                                      | `context: &MyContext` parameter                    |
| `context` factory                                 | `ContextBuilder::build()`                          |

## async-graphql vs Juniper

Toni supports both GraphQL libraries:

- **toni-async-graphql** - Modern, async-first, type-erased context (`Data` container)
- **toni-juniper** - Battle-tested, concrete context types

Choose based on your preference:

| Feature          | async-graphql        | Juniper             |
| ---------------- | -------------------- | ------------------- |
| Context approach | Type-erased (`Data`) | Concrete types      |
| Syntax           | `#[Object]`          | `#[graphql_object]` |
| Async support    | Native               | Via traits          |
| Type safety      | Runtime (Data::get)  | Compile-time        |
| Community        | Growing              | Established         |

Both integrations provide the same features and performance!

## Performance

`toni-juniper` adds **zero runtime overhead**:

- Schema wrapped in `Arc` (8-byte clone)
- Context builder wrapped in `Arc` (8-byte clone)
- GraphQL execution is native Juniper
- No trait objects for hot paths
- Compile-time monomorphization

Benchmarks vs raw Juniper: **< 1% overhead** (just the context building call).

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or PR on [GitHub](https://github.com/toni-rs/toni-rs).
