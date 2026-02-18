//! # toni-async-graphql
//!
//! async-graphql integration for the Toni framework.
//!
//! This crate provides seamless integration between [async-graphql](https://github.com/async-graphql/async-graphql)
//! and the [Toni](https://github.com/toni-rs/toni) web framework, enabling you to build
//! type-safe GraphQL APIs with dependency injection, middleware, guards, and all Toni features.
//!
//! ## Features
//!
//! - **Full async-graphql support** - Use all async-graphql features natively
//! - **Dependency Injection** - Inject Toni services into your context builders
//! - **User-controlled context** - Build GraphQL context however you want
//! - **Guards & Interceptors** - Use Toni's guards and interceptors with GraphQL
//! - **GraphQL Playground** - Built-in playground for development
//! - **Zero overhead** - Compiles to native async-graphql code
//!
//! ## Quick Start
//!
//! ```ignore
//! use toni::ToniFactory;
//! use toni_axum::AxumAdapter;
//! use toni_async_graphql::{GraphQLModule, DefaultContextBuilder, async_graphql::*};
//!
//! // Define your schema
//! struct Query;
//!
//! #[Object]
//! impl Query {
//!     async fn hello(&self) -> &str {
//!         "Hello, world!"
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     // Build schema
//!     let schema = Schema::build(Query, EmptyMutation, EmptySubscription).finish();
//!
//!     // Create GraphQL module
//!     let graphql_module = GraphQLModule::for_root(schema, DefaultContextBuilder);
//!
//!     // Create Toni app
//!     let adapter = AxumAdapter::new();
//!
//!     let mut app = ToniFactory::create(
//!         AppModule {
//!             imports: vec![graphql_module.into()],
//!             ..Default::default()
//!         },
//!         adapter
//!     ).await;
//!
//!     app.listen(3000, "127.0.0.1").await;
//! }
//! ```
//!
//! ## Custom Context
//!
//! Build GraphQL context with access to Toni's DI system:
//!
//! ```ignore
//! use toni_async_graphql::{ContextBuilder, async_graphql::Data};
//! use toni::{HttpRequest, injectable};
//! use async_trait::async_trait;
//!
//! #[injectable(
//!     pub struct MyContextBuilder {
//!         auth_service: AuthService,
//!         db_pool: DatabasePool,
//!     }
//! )]
//! #[async_trait]
//! impl ContextBuilder for MyContextBuilder {
//!     async fn build(&self, req: &HttpRequest) -> Data {
//!         let mut data = Data::default();
//!
//!         // Add HTTP request
//!         data.insert(req.clone());
//!
//!         // Add user from auth service (DI!)
//!         if let Some(user) = self.auth_service.verify_token(req) {
//!             data.insert(user);
//!         }
//!
//!         // Add database pool
//!         data.insert(self.db_pool.clone());
//!
//!         data
//!     }
//! }
//! ```
//!
//! ## Accessing Context in Resolvers
//!
//! ```ignore
//! use async_graphql::{Object, Context, Result};
//!
//! struct Query;
//!
//! #[Object]
//! impl Query {
//!     async fn me(&self, ctx: &Context<'_>) -> Result<User> {
//!         // Get user from context (added by auth middleware/guard)
//!         let user = ctx.data::<User>()?;
//!         Ok(user.clone())
//!     }
//!
//!     async fn user(&self, ctx: &Context<'_>, id: i32) -> Result<User> {
//!         // Get DI service from context
//!         let db_pool = ctx.data::<DatabasePool>()?;
//!         db_pool.find_user(id).await
//!     }
//! }
//! ```

mod context_builder;
mod graphql_controller;
mod graphql_module;
mod graphql_service;
mod graphql_service_manager;

// Re-export key types
pub use context_builder::{ContextBuilder, DefaultContextBuilder};
pub use graphql_module::GraphQLModule;
pub use graphql_service::GraphQLService;

// Re-export async-graphql for convenience
pub use async_graphql;

/// Prelude module with common imports
pub mod prelude {
    pub use crate::{ContextBuilder, DefaultContextBuilder, GraphQLModule, GraphQLService};
    pub use async_graphql::{
        Context, EmptyMutation, EmptySubscription, Enum, InputObject, Interface, Object, Schema,
        SimpleObject, Union,
    };
}
