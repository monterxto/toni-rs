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

/// Extracts a value from an HTTP request.
///
/// `from_request` receives a shared reference to the full request (including
/// the buffered body via `req.body()`). Call `req.into_parts()` in middleware
/// when you need to replace parts of the request before passing it downstream.
pub trait FromRequest: Sized {
    type Error: std::fmt::Display;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error>;
}
