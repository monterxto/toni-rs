use async_graphql::Data;
use async_trait::async_trait;
use toni::RequestPart;

/// Trait for building GraphQL context from HTTP requests.
///
/// Implement this trait to customize how GraphQL context is built for each request.
/// The context builder can inject Toni services via dependency injection.
///
/// # Examples
///
/// ## Simple context (no DI)
///
/// ```rust
/// use toni_async_graphql::{ContextBuilder, async_graphql::Data};
/// use toni::RequestPart;
/// use async_trait::async_trait;
///
/// struct SimpleContext;
///
/// #[async_trait]
/// impl ContextBuilder for SimpleContext {
///     async fn build(&self, _req: &RequestPart) -> Data {
///         Data::default()
///     }
/// }
/// ```
///
/// ## Context with DI services
///
/// ```ignore
/// use toni_async_graphql::{ContextBuilder, async_graphql::Data};
/// use toni::{RequestPart, injectable};
/// use async_trait::async_trait;
///
/// #[injectable(
///     pub struct MyContextBuilder {
///         auth_service: AuthService,
///         db_pool: DatabasePool,
///     }
/// )]
/// #[async_trait]
/// impl ContextBuilder for MyContextBuilder {
///     async fn build(&self, req: &RequestPart) -> Data {
///         let mut data = Data::default();
///
///         // Add HTTP request
///         data.insert(req.clone());
///
///         // Extract user from auth service
///         if let Some(user) = self.auth_service.verify_token(req) {
///             data.insert(user);
///         }
///
///         // Add database pool
///         data.insert(self.db_pool.clone());
///
///         data
///     }
/// }
/// ```
#[async_trait]
pub trait ContextBuilder: Send + Sync + 'static {
    /// Build GraphQL context from an HTTP request.
    ///
    /// This method is called before executing each GraphQL query.
    /// You can extract data from the request, use injected services,
    /// and populate the context with any data your resolvers need.
    ///
    /// # Arguments
    ///
    /// * `req` - The incoming HTTP request
    ///
    /// # Returns
    ///
    /// An `async_graphql::Data` container with your context data.
    async fn build(&self, req: &RequestPart) -> Data;
}

/// Default context builder that creates an empty context.
///
/// Use this when you don't need any context data.
#[derive(Clone)]
pub struct DefaultContextBuilder;

#[async_trait]
impl ContextBuilder for DefaultContextBuilder {
    async fn build(&self, _req: &RequestPart) -> Data {
        Data::default()
    }
}
