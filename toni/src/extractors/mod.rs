//! Extractors for request data
//!
//! Extractors provide a type-safe way to extract data from HTTP requests.
//! They work by implementing the `FromRequest` trait.
//!
//! # Example
//!
//! ```rust,ignore
//! use toni::extractors::{Path, Query, Json, Validated};
//!
//! #[get("/users/:id")]
//! fn get_user(&self, Path(id): Path<i32>) -> String {
//!     format!("User {}", id)
//! }
//!
//! #[post("/users")]
//! fn create_user(&self, Json(dto): Validated<Json<CreateUserDto>>) -> String {
//!     format!("Created {}", dto.name)
//! }
//! ```

mod body;
mod bytes;
mod json;
mod path;
mod query;
mod validated;

pub use body::Body;
pub use bytes::Bytes;
pub use json::Json;
pub use path::Path;
pub use query::Query;
pub use validated::Validated;

use crate::http_helpers::HttpRequest;

/// Trait for types that can be extracted from an HTTP request
pub trait FromRequest: Sized {
    /// The error type returned if extraction fails
    type Error: std::fmt::Display;

    /// Extract self from the request
    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error>;
}
