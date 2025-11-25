use super::{Controller, Provider};
use crate::middleware::{IntoRoutePattern, RoutePattern};
use crate::traits_helpers::middleware::{Middleware, MiddlewareConfiguration};
use std::sync::Arc;

pub trait ModuleMetadata {
    fn get_id(&self) -> String;
    fn get_name(&self) -> String;
    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>>;
    fn controllers(&self) -> Option<Vec<Box<dyn Controller>>>;
    fn providers(&self) -> Option<Vec<Box<dyn Provider>>>;
    fn exports(&self) -> Option<Vec<String>>;

    /// Returns true if this module is global (exports available everywhere)
    fn is_global(&self) -> bool {
        false // Default: non-global
    }

    /// Configure middleware for this module
    fn configure_middleware(&self, _consumer: &mut MiddlewareConsumer) {
        // Default: do nothing
    }

    /// Mark this module as global, making its exports available everywhere
    fn global(self) -> GlobalModuleWrapper<Self>
    where
        Self: Sized,
    {
        GlobalModuleWrapper { inner: self }
    }
}

/// Wrapper that makes any module global by overriding is_global()
pub struct GlobalModuleWrapper<T: ModuleMetadata> {
    inner: T,
}

impl<T: ModuleMetadata> ModuleMetadata for GlobalModuleWrapper<T> {
    fn get_id(&self) -> String {
        self.inner.get_id()
    }

    fn get_name(&self) -> String {
        self.inner.get_name()
    }

    fn is_global(&self) -> bool {
        true // Always return true for global wrapper
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        self.inner.imports()
    }

    fn controllers(&self) -> Option<Vec<Box<dyn Controller>>> {
        self.inner.controllers()
    }

    fn providers(&self) -> Option<Vec<Box<dyn Provider>>> {
        self.inner.providers()
    }

    fn exports(&self) -> Option<Vec<String>> {
        self.inner.exports()
    }

    fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
        self.inner.configure_middleware(consumer)
    }
}

/// Builder for configuring middleware in modules
///
/// This provides a fluent API for configuring middleware with route patterns.
/// Used within the `configure_middleware` method of your modules.
///
/// # Example
/// ```ignore
/// #[module(controllers: [UserController])]
/// impl UserModule {
///     fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
///         // Apply logger to all routes
///         consumer
///             .apply(MyLoggerMiddleware::new())
///             .for_routes(vec!["/users/*"]);
///
///         // Apply auth to specific routes, excluding public endpoints
///         consumer
///             .apply(MyAuthMiddleware::new())
///             .for_routes(vec!["/users/*"])
///             .exclude(vec!["/users/public/*"]);
///
///         // Multiple middleware can be applied to the same routes
///         consumer
///             .apply(MyRateLimitMiddleware::new(100, 60000))
///             .for_routes(vec![("/users/create", "POST")]);
///     }
/// }
/// ```
///
/// # Route Patterns
///
/// Supports glob-like patterns and HTTP method filtering:
/// - `/users` - Exact match, all HTTP methods
/// - `/api/*` - All routes starting with /api/, all HTTP methods
/// - `("/users", "POST")` - Only POST requests to /users
/// - `("/api/*", ["GET", "POST"])` - Only GET and POST to /api/*
pub struct MiddlewareConsumer {
    configurations: Vec<MiddlewareConfiguration>,
    current_middleware: Vec<Arc<dyn Middleware>>,
    current_middleware_tokens: Vec<String>,
    current_includes: Vec<RoutePattern>,
    current_excludes: Vec<RoutePattern>,
}

impl MiddlewareConsumer {
    pub fn new() -> Self {
        Self {
            configurations: Vec::new(),
            current_middleware: Vec::new(),
            current_middleware_tokens: Vec::new(),
            current_includes: Vec::new(),
            current_excludes: Vec::new(),
        }
    }

    /// Apply middleware to routes
    ///
    /// Returns a proxy that requires you to specify routes via `.for_routes()` or `.for_route()`.
    ///
    /// # Example
    /// ```ignore
    /// // Single middleware
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .for_routes(vec!["/api/*"]);
    ///
    /// // Multiple middleware on same routes
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .apply_also(MyAuthMiddleware::new())
    ///     .for_routes(vec!["/api/*"]);
    /// ```
    pub fn apply<M>(&mut self, middleware: M) -> MiddlewareConfigProxy<'_>
    where
        M: Middleware + 'static,
    {
        self.current_middleware.push(Arc::new(middleware));
        MiddlewareConfigProxy { consumer: self }
    }

    /// Apply middleware using token-based DI resolution
    ///
    /// This allows middleware to be resolved from the DI container, enabling
    /// middleware to have constructor dependencies injected.
    ///
    /// # Example
    /// ```ignore
    /// // Middleware with DI dependencies
    /// consumer
    ///     .apply_token::<RequestTrackingMiddleware>()
    ///     .for_routes(vec!["/api/*"]);
    ///
    /// // Multiple DI middleware on same routes
    /// consumer
    ///     .apply_token::<LoggerMiddleware>()
    ///     .apply_token_also::<AuthMiddleware>()
    ///     .for_routes(vec!["/api/*"]);
    /// ```
    pub fn apply_token<M>(&mut self) -> MiddlewareConfigProxy<'_>
    where
        M: 'static,
    {
        let token = std::any::type_name::<M>().to_string();
        self.current_middleware_tokens.push(token);
        MiddlewareConfigProxy { consumer: self }
    }

    /// Finalize current middleware configuration
    fn finalize_current(&mut self) {
        if !self.current_middleware.is_empty() || !self.current_middleware_tokens.is_empty() {
            let config = MiddlewareConfiguration {
                middleware: std::mem::take(&mut self.current_middleware),
                middleware_tokens: std::mem::take(&mut self.current_middleware_tokens),
                include_patterns: std::mem::take(&mut self.current_includes),
                exclude_patterns: std::mem::take(&mut self.current_excludes),
            };
            self.configurations.push(config);
        }
    }

    /// Get all configurations
    pub fn build(mut self) -> Vec<MiddlewareConfiguration> {
        self.finalize_current();
        self.configurations
    }
}

impl Default for MiddlewareConsumer {
    fn default() -> Self {
        Self::new()
    }
}

/// Proxy type returned by `.apply()` that enforces route specification
///
/// This type-state pattern ensures you cannot forget to call `.for_routes()`, `.for_route()`,
/// or `.done()` after applying middleware.
///
/// # Methods
/// - `.apply_also()` - Add another middleware to the same configuration
/// - `.for_route()` - Add a single route (chainable, returns proxy)
/// - `.for_routes()` - Add multiple routes and finalize (returns consumer)
/// - `.exclude_route()` - Exclude a single route (chainable, returns proxy)
/// - `.exclude()` - Exclude multiple routes (chainable, returns proxy)
/// - `.done()` - Finalize configuration (returns consumer)
#[must_use = "Middleware proxy must call .for_routes(), .for_route(), or .done() to complete configuration"]
pub struct MiddlewareConfigProxy<'a> {
    consumer: &'a mut MiddlewareConsumer,
}

impl<'a> MiddlewareConfigProxy<'a> {
    /// Add another middleware to the same configuration
    ///
    /// This allows you to group multiple middleware that should apply to the same routes.
    ///
    /// # Example
    /// ```ignore
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .apply_also(MyAuthMiddleware::new())
    ///     .apply_also(MyCorsMiddleware::new())
    ///     .for_routes(vec!["/api/*"]);
    /// ```
    pub fn apply_also<M>(self, middleware: M) -> Self
    where
        M: Middleware + 'static,
    {
        self.consumer.current_middleware.push(Arc::new(middleware));
        self
    }

    /// Add another middleware via token-based DI resolution
    ///
    /// # Example
    /// ```ignore
    /// consumer
    ///     .apply_token::<LoggerMiddleware>()
    ///     .apply_token_also::<AuthMiddleware>()
    ///     .apply_token_also::<CorsMiddleware>()
    ///     .for_routes(vec!["/api/*"]);
    /// ```
    pub fn apply_token_also<M>(self) -> Self
    where
        M: 'static,
    {
        let token = std::any::type_name::<M>().to_string();
        self.consumer.current_middleware_tokens.push(token);
        self
    }

    /// Specify a single route to apply middleware to
    ///
    /// Returns the proxy so you can chain more routes or exclusions before finalizing.
    /// Call `.done()` when finished to finalize and return the consumer.
    ///
    /// # Accepted Types
    /// - `&str` - Path pattern, all HTTP methods: `"/api/*"`
    /// - `(&str, &str)` - Path with single method: `("/users", "POST")`
    /// - `(&str, [&str; N])` - Path with multiple methods: `("/api/*", ["GET", "POST"])`
    /// - `(&str, &[&str; N])` - Path with ref to array: `("/api/*", &["GET", "POST"])`
    /// - `(&str, Vec<&str>)` - Path with methods vec: `("/api/*", vec!["GET", "POST"])`
    ///
    /// # Examples
    /// ```ignore
    /// // Chain multiple routes
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .for_route("/api/*")
    ///     .for_route("/admin/*")
    ///     .done();
    ///
    /// // Mix with exclusions
    /// consumer
    ///     .apply(MyAuthMiddleware::new())
    ///     .for_route("/api/*")
    ///     .exclude_route("/api/public/*")
    ///     .done();
    /// ```
    pub fn for_route<T: IntoRoutePattern>(self, pattern: T) -> Self {
        self.consumer
            .current_includes
            .push(pattern.into_route_pattern());
        self
    }

    /// Specify multiple routes to apply middleware to
    ///
    /// **This method finalizes the middleware configuration** and returns the consumer,
    /// allowing you to chain another `.apply()` call.
    ///
    /// You can also call this with an empty `vec![]` to finalize a configuration
    /// that was built using `.for_route()` chains.
    ///
    /// # Accepted Types
    /// Each element in the `Vec` can be:
    /// - `&str` - Path pattern, all HTTP methods: `"/api/*"`
    /// - `(&str, &str)` - Path with single method: `("/users", "POST")`
    /// - `(&str, [&str; N])` - Path with multiple methods: `("/api/*", ["GET", "POST"])`
    /// - `(&str, &[&str; N])` - Path with ref to array: `("/api/*", &["GET", "POST"])`
    /// - `(&str, Vec<&str>)` - Path with methods vec: `("/api/*", vec!["GET", "POST"])`
    ///
    /// # Examples
    /// ```ignore
    /// // Multiple simple paths (all methods)
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .for_routes(vec!["/api/*", "/admin/*", "/users/*"]);
    ///
    /// // Multiple routes with HTTP method arrays (same size)
    /// consumer
    ///     .apply(MyAuthMiddleware::new())
    ///     .for_routes(vec![
    ///         ("/api/users/*", ["GET", "POST"]),
    ///         ("/api/posts/*", ["GET", "POST"]),
    ///     ]);
    ///
    /// // Different-sized arrays? Use Vec instead
    /// consumer
    ///     .apply(MyCorsMiddleware::new())
    ///     .for_routes(vec![
    ///         ("/api/users/*", vec!["GET", "POST"]),
    ///         ("/api/admin/*", vec!["GET", "POST", "DELETE"]),
    ///     ]);
    ///
    /// // Finalize a .for_route() chain (empty vec is fine)
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .for_route("/api/*")
    ///     .for_route("/admin/*")
    ///     .for_routes(vec![]);
    ///
    /// // Mix types in same vec (use vec![] for "all methods")
    /// consumer
    ///     .apply(MyCorsMiddleware::new())
    ///     .for_routes(vec![
    ///         ("/api/public/*", vec![]),  // All methods
    ///         ("/api/admin/*", vec!["GET", "POST", "DELETE"]),  // Specific methods
    ///     ]);
    /// ```
    pub fn for_routes<T: IntoRoutePattern>(self, patterns: Vec<T>) -> &'a mut MiddlewareConsumer {
        let mut new_patterns: Vec<RoutePattern> = patterns
            .into_iter()
            .map(|p| p.into_route_pattern())
            .collect();

        self.consumer.current_includes.append(&mut new_patterns);

        // Finalize the configuration now that routes are specified
        self.consumer.finalize_current();
        self.consumer
    }

    /// Exclude a single route from middleware
    ///
    /// Returns the proxy, so you can continue chaining exclusions or call `.for_routes()`.
    ///
    /// # Accepted Types
    /// - `&str` - Path pattern, all HTTP methods: `"/api/public/*"`
    /// - `(&str, &str)` - Path with single method: `("/users/login", "POST")`
    /// - `(&str, [&str; N])` - Path with multiple methods: `("/api/public/*", ["GET", "POST"])`
    /// - `(&str, &[&str; N])` - Path with ref to array: `("/api/public/*", &["GET", "POST"])`
    /// - `(&str, Vec<&str>)` - Path with methods vec: `("/health", vec!["GET"])`
    ///
    /// # Examples
    /// ```ignore
    /// // Exclude public routes from auth
    /// consumer
    ///     .apply(MyAuthMiddleware::new())
    ///     .exclude_route("/api/public/*")
    ///     .exclude_route("/api/health")
    ///     .for_routes(vec!["/api/*"]);
    ///
    /// // Exclude specific method on a route
    /// consumer
    ///     .apply(MyRateLimitMiddleware::new(100, 60000))
    ///     .exclude_route(("/api/health", "GET"))
    ///     .for_routes(vec!["/api/*"]);
    /// ```
    pub fn exclude_route<T: IntoRoutePattern>(self, pattern: T) -> Self {
        self.consumer
            .current_excludes
            .push(pattern.into_route_pattern());
        self
    }

    /// Exclude multiple routes from middleware
    ///
    /// Returns the proxy, so you can continue chaining exclusions or call `.for_routes()`.
    ///
    /// # Accepted Types
    /// Each element in the `Vec` can be:
    /// - `&str` - Path pattern, all HTTP methods: `"/api/public/*"`
    /// - `(&str, &str)` - Path with single method: `("/users/login", "POST")`
    /// - `(&str, [&str; N])` - Path with multiple methods: `("/api/public/*", ["GET", "POST"])`
    /// - `(&str, &[&str; N])` - Path with ref to array: `("/api/public/*", &["GET", "POST"])`
    /// - `(&str, Vec<&str>)` - Path with methods vec: `("/health", vec!["GET"])`
    ///
    /// # Examples
    /// ```ignore
    /// // Exclude multiple public routes from auth
    /// consumer
    ///     .apply(MyAuthMiddleware::new())
    ///     .exclude(vec!["/api/public/*", "/api/health", "/api/status"])
    ///     .for_routes(vec!["/api/*"]);
    ///
    /// // Exclude routes with method arrays (same size)
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .exclude(vec![
    ///         ("/api/health", ["GET", "HEAD"]),
    ///         ("/api/status", ["GET", "HEAD"]),
    ///     ])
    ///     .for_routes(vec!["/api/*"]);
    ///
    /// // Different-sized arrays? Use Vec instead
    /// consumer
    ///     .apply(MyRateLimitMiddleware::new(100, 60000))
    ///     .exclude(vec![
    ///         ("/api/health", vec!["GET", "HEAD"]),
    ///         ("/api/metrics", vec!["GET"]),
    ///     ])
    ///     .for_routes(vec!["/api/*"]);
    /// ```
    pub fn exclude<T: IntoRoutePattern>(self, patterns: Vec<T>) -> Self {
        let mut new_patterns: Vec<RoutePattern> = patterns
            .into_iter()
            .map(|p| p.into_route_pattern())
            .collect();

        self.consumer.current_excludes.append(&mut new_patterns);
        self
    }

    /// Finalize the middleware configuration
    ///
    /// This method finalizes the current middleware configuration and returns the consumer,
    /// allowing you to chain another `.apply()` call. Use this when you've built up routes
    /// using `.for_route()` and `.exclude_route()` chains.
    ///
    /// # Example
    /// ```ignore
    /// // Chain routes then finalize
    /// consumer
    ///     .apply(MyLoggerMiddleware::new())
    ///     .for_route("/api/*")
    ///     .for_route("/admin/*")
    ///     .exclude_route("/api/health")
    ///     .done();
    ///
    /// // Then continue with another middleware
    /// consumer
    ///     .apply(MyAuthMiddleware::new())
    ///     .for_route("/admin/*")
    ///     .done();
    /// ```
    pub fn done(self) -> &'a mut MiddlewareConsumer {
        self.consumer.finalize_current();
        self.consumer
    }
}
