use serde_json::Value;

use super::{Body, HttpResponse};

/// Converts a value into an [`HttpResponse`].
///
/// Implement this to make a type returnable from a controller handler.
/// All built-in types (`Body`, `String`, `&str`, `serde_json::Value`, etc.)
/// are already covered.
pub trait IntoResponse {
    fn into_response(self) -> HttpResponse;
}

impl IntoResponse for HttpResponse {
    fn into_response(self) -> HttpResponse {
        self
    }
}

impl IntoResponse for Body {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            body: Some(self),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for u16 {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            status: self,
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for Vec<(String, String)> {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            headers: self,
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for (u16, Body) {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            status: self.0,
            body: Some(self.1),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for Value {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            body: Some(Body::json(self)),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for String {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            body: Some(Body::text(self)),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> HttpResponse {
        HttpResponse {
            body: Some(Body::text(self)),
            ..HttpResponse::new()
        }
    }
}

impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse,
    E: IntoResponse,
{
    fn into_response(self) -> HttpResponse {
        match self {
            Ok(value) => value.into_response(),
            Err(error) => error.into_response(),
        }
    }
}
