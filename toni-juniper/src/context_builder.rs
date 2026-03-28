use async_trait::async_trait;
use toni::RequestPart;

/// Trait for building GraphQL context from HTTP request metadata.
///
/// Unlike async-graphql's type-erased `Data` approach, Juniper uses
/// concrete context types. This trait allows users to build their
/// context however they want, with full access to Toni's DI system.
///
/// # Example
///
/// ```ignore
/// use async_trait::async_trait;
/// use toni::RequestPart;
/// use toni_juniper::ContextBuilder;
/// use juniper::Context as JuniperContext;
///
/// #[derive(Clone)]
/// struct MyContext {
///     user_id: Option<i32>,
///     db: DatabaseService,
/// }
///
/// impl JuniperContext for MyContext {}
///
/// #[injectable(
///     pub struct _MyContextBuilder {
///         auth_service: _AuthService,
///         db_service: _DatabaseService,
///     }
/// )]
/// #[async_trait]
/// impl ContextBuilder for _MyContextBuilder {
///     type Context = MyContext;
///
///     async fn build(&self, req: &RequestPart) -> Self::Context {
///         MyContext {
///             user_id: self.auth_service.verify_token(req),
///             db: self.db_service.clone(),
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait ContextBuilder: Send + Sync + 'static {
    /// The concrete context type for Juniper.
    /// Must implement `juniper::Context` marker trait.
    type Context: juniper::Context + Send + Sync + Clone + 'static;

    /// Build GraphQL context from HTTP request metadata.
    ///
    /// Called on every GraphQL request. Can inject Toni services
    /// and perform authentication, database setup, etc.
    async fn build(&self, req: &RequestPart) -> Self::Context;
}

/// Default context builder that provides an empty context.
///
/// Useful for simple GraphQL APIs that don't need request context.
#[derive(Clone)]
pub struct DefaultContextBuilder;

/// Default empty context type
#[derive(Clone)]
pub struct DefaultContext;

impl juniper::Context for DefaultContext {}

#[async_trait]
impl ContextBuilder for DefaultContextBuilder {
    type Context = DefaultContext;

    async fn build(&self, _req: &RequestPart) -> Self::Context {
        DefaultContext
    }
}
