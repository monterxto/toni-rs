//! Static, type-safe storage for route-level configuration.
//!
//! Unlike `Extensions` which stores request-scoped data, `RouteMetadata` stores
//! configuration that is defined once per route and shared across all requests.
//!
//! # Example
//!
//! ```
//! use toni::http_helpers::RouteMetadata;
//!
//! #[derive(Clone)]
//! struct Roles(Vec<&'static str>);
//!
//! let mut metadata = RouteMetadata::new();
//! metadata.insert(Roles(vec!["admin", "moderator"]));
//!
//! // Later, in a guard:
//! if let Some(Roles(required)) = metadata.get::<Roles>() {
//!     // Check user has required roles
//! }
//! ```

use super::Extensions;

/// Route-level metadata storage.
///
/// Populated once at route registration, shared immutably across all requests.
/// Guards and interceptors read from this to get route-specific configuration.
#[derive(Clone, Default)]
pub struct RouteMetadata {
    data: Extensions,
}

impl RouteMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        self.data.insert(val)
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.data.get()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl std::fmt::Debug for RouteMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteMetadata").finish()
    }
}
