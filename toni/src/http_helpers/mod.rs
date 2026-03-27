#[path = "body.enum.rs"]
mod body;
pub use self::body::{Body, BoxBody};
pub use bytes::Bytes;

#[path = "http_response.enum.rs"]
mod http_response;
pub use self::http_response::{HttpResponse, HttpResponseBuilder, HttpResponseDefault};

mod path_params;
pub use self::path_params::PathParams;

#[path = "http_request.struct.rs"]
mod http_request;
pub use self::http_request::HttpRequest;

/// Type alias for the metadata-only half of a split request.
///
/// Middleware that calls `req.into_parts()` receives `(RequestParts, Bytes)`.
/// Extractors that only need headers/method/URI work on `&RequestParts`.
pub use http::request::Parts as RequestParts;

#[path = "http_method.enum.rs"]
mod http_method;
pub use self::http_method::HttpMethod;

#[path = "into_response.rs"]
mod into_response;
pub use self::into_response::IntoResponse;

mod extensions;
pub use self::extensions::Extensions;

mod route_metadata;
pub use self::route_metadata::RouteMetadata;
