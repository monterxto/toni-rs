mod chain;
mod route_pattern;
pub use chain::MiddlewareChain;
pub use route_pattern::{IntoRoutePattern, RoutePattern};

mod module_middleware;
pub use module_middleware::MiddlewareManager;

// Re-export core traits
pub use crate::traits_helpers::middleware::{
    FunctionalMiddleware, Middleware, MiddlewareConfiguration, MiddlewareFn, MiddlewareResult,
    NextHandle,
};
