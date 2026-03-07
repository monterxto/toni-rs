/*!
# toni-juniper

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

## Quick Start

```ignore
use juniper::{EmptyMutation, EmptySubscription, RootNode, graphql_object};
use toni::{module, ToniFactory, HttpAdapter};
use toni_axum::AxumAdapter;
use toni_juniper::{GraphQLModule, DefaultContextBuilder, DefaultContext};

struct Query;

#[graphql_object(context = DefaultContext)]
impl Query {
    fn hello() -> &'static str {
        "Hello, world!"
    }
}

fn build_graphql_module() -> GraphQLModule<Query, EmptyMutation<DefaultContext>, EmptySubscription<DefaultContext>, DefaultContextBuilder> {
    let schema = RootNode::new(
        Query,
        EmptyMutation::new(),
        EmptySubscription::new(),
    );
    GraphQLModule::for_root(schema, DefaultContextBuilder)
}

#[module(
    imports: [build_graphql_module()],
    controllers: [],
    providers: [],
    exports: []
)]
impl AppModule {}

#[tokio::main]
async fn main() {
    let adapter = AxumAdapter::new();

    let mut app = ToniFactory::create(AppModule::module_definition(), adapter)
        .await;

    app.listen(3000, "127.0.0.1").await;
}
```

Visit `http://localhost:3000/graphql` to use GraphQL Playground!

## Custom Context with DI

Build GraphQL context with access to Toni's dependency injection:

```ignore
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

```ignore
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

```ignore
let graphql_module = GraphQLModule::for_root(schema, context_builder)
    .with_path("/api/graphql");
```

### Enable/Disable Playground

```ignore
let graphql_module = GraphQLModule::for_root(schema, context_builder)
    .with_playground(false);  // Disable in production
```

By default, playground is enabled in debug builds and disabled in release builds.
*/

mod context_builder;
mod graphql_controller;
mod graphql_module;
mod graphql_service;
mod graphql_service_manager;

pub use context_builder::{ContextBuilder, DefaultContext, DefaultContextBuilder};
pub use graphql_module::GraphQLModule;
pub use graphql_service::GraphQLService;

// Re-export juniper for convenience
pub use juniper;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::context_builder::{ContextBuilder, DefaultContext, DefaultContextBuilder};
    pub use crate::graphql_module::GraphQLModule;
    pub use crate::graphql_service::GraphQLService;
    pub use juniper::{
        graphql_object, graphql_value, EmptyMutation, EmptySubscription, FieldResult, RootNode,
    };
}
