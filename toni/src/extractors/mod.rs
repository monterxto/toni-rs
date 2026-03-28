//! Extractors for request data
//!
//! Two extraction traits cover the two cases:
//!
//! - [`FromRequestParts`] — sync, metadata only (headers, path params, query
//!   params). Implemented by `Path`, `Query`, and any extractor that doesn't
//!   touch the body.
//!
//! - [`FromRequest`] — async, consumes the request. Implemented by `Json`,
//!   `Bytes`, `Body`, and `BodyStream`. Only one `FromRequest` extractor that
//!   actually reads the body can appear per handler because the body is
//!   single-use.

mod body;
mod body_stream;
mod bytes;
mod json;
mod path;
mod query;
mod validated;

pub use body::Body;
pub use body_stream::BodyStream;
pub use bytes::Bytes;
pub use json::Json;
pub use path::Path;
pub use query::Query;
pub use validated::Validated;

use crate::http_helpers::{HttpRequest, RequestPart};

/// Extracts a value from request metadata (method, URI, headers, extensions,
/// path params, query params). Sync and non-consuming — safe to call multiple
/// times per request without touching the body.
pub trait FromRequestParts: Sized {
    type Error: std::fmt::Display;

    fn from_request_parts(parts: &RequestPart) -> Result<Self, Self::Error>;
}

/// Extracts a value from the full request, potentially consuming the body.
/// Async and single-use for body-consuming implementations — only one
/// body-reading extractor may appear per handler.
///
/// All [`FromRequestParts`] types automatically implement this trait via a
/// blanket impl that ignores the body.
pub trait FromRequest: Sized {
    type Error: std::fmt::Display + Send + Sync + 'static;

    fn from_request(
        req: HttpRequest,
    ) -> impl std::future::Future<Output = Result<Self, Self::Error>> + Send;
}

impl<T: FromRequestParts> FromRequest for T
where
    <T as FromRequestParts>::Error: Send + Sync + 'static,
{
    type Error = <T as FromRequestParts>::Error;

    async fn from_request(req: HttpRequest) -> Result<Self, Self::Error> {
        let (parts, _) = req.0.into_parts();
        T::from_request_parts(&parts)
    }
}
