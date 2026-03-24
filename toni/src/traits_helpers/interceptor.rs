use async_trait::async_trait;

use crate::injector::Context;

/// Represents the next handler in the interceptor chain.
///
/// This trait allows interceptors to
/// control when (or if) the next handler in the chain executes.
///
/// # Note
/// The `run` method consumes `self` (via `Box<Self>`), so it can only be called once.
/// This prevents accidentally calling the handler multiple times.
#[async_trait]
pub trait InterceptorNext: Send {
    /// Execute the next handler in the chain
    ///
    /// # Parameters
    /// - `context`: Mutable context containing request, response, and other data
    async fn run(self: Box<Self>, context: &mut Context);
}

/// Interceptor trait for wrapping route handler execution
///
/// Interceptors can perform operations before and after the handler executes,
/// transform responses, implement caching, handle errors, and more.
///
/// # Examples
///
/// Basic timing interceptor:
/// ```rust
/// use async_trait::async_trait;
/// use std::time::Instant;
/// use toni::traits_helpers::{Interceptor, InterceptorNext};
/// use toni::injector::Context;
///
/// struct TimingInterceptor;
///
/// #[async_trait]
/// impl Interceptor for TimingInterceptor {
///     async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
///         let start = Instant::now();
///         println!("Request starting");
///
///         next.run(context).await;  // Execute handler
///
///         println!("Request took {:?}", start.elapsed());
///     }
/// }
/// ```
///
/// Caching interceptor (conditional execution):
/// ```rust
/// # use async_trait::async_trait;
/// # use toni::traits_helpers::{Interceptor, InterceptorNext};
/// # use toni::injector::Context;
/// # use toni::HttpResponse;
/// # struct Cache;
/// # impl Cache {
/// #     fn get(&self, _key: &str) -> Option<HttpResponse> { None }
/// #     fn set(&self, _key: String, _val: HttpResponse) {}
/// # }
/// struct CacheInterceptor {
///     cache: Cache,
/// }
///
/// #[async_trait]
/// impl Interceptor for CacheInterceptor {
///     async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
///         let key = context.switch_to_http()
///             .map(|(req, _)| req.uri.clone())
///             .unwrap_or_default();
///
///         if let Some(cached) = self.cache.get(&key) {
///             context.set_response(Box::new(cached));
///             // Skip handler execution by NOT calling next.run()!
///             return;
///         }
///
///         next.run(context).await;
///
///         if let Some(response) = context.get_response_ref() {
///             self.cache.set(key, response.clone());
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait Interceptor: Send + Sync {
    /// Intercept the handler execution
    ///
    /// # Parameters
    /// - `context`: Mutable context containing request, response, and other data
    /// - `next`: The next handler in the chain. Call `next.run(context).await` to execute it.
    ///
    /// # Execution Flow
    /// - Code before `next.run(context).await` runs before the handler
    /// - `next.run(context).await` executes the handler
    /// - Code after `next.run(context).await` runs after the handler
    ///
    /// # Note
    /// You can choose NOT to call `next.run()` to skip handler execution entirely.
    /// This is useful for caching, circuit breakers, or early returns.
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>);
}
