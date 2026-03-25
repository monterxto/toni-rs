use std::fmt::Debug;

use serde_json::Value;

use super::{Body, HttpResponse};

pub trait ToResponse: Debug {
    type Response;

    fn to_response(&self) -> Self::Response;
}

impl ToResponse for HttpResponse {
    type Response = Self;

    fn to_response(&self) -> Self {
        self.clone()
    }
}

impl ToResponse for Body {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(self.clone()),
            ..HttpResponse::new()
        }
    }
}

impl ToResponse for u16 {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            status: *self,
            ..HttpResponse::new()
        }
    }
}

impl ToResponse for Vec<(String, String)> {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            headers: self.clone(),
            ..HttpResponse::new()
        }
    }
}

impl ToResponse for (u16, Body) {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(self.1.clone()),
            status: self.0,
            ..HttpResponse::new()
        }
    }
}

impl ToResponse for Value {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(Body::json(self.clone())),
            ..HttpResponse::new()
        }
    }
}

impl ToResponse for String {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(Body::text(self.clone())),
            ..HttpResponse::new()
        }
    }
}

impl ToResponse for &'static str {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(Body::text(*self)),
            ..HttpResponse::new()
        }
    }
}

impl<T, E> ToResponse for Result<T, E>
where
    T: ToResponse<Response = HttpResponse>,
    E: ToResponse<Response = HttpResponse>,
{
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        match self {
            Ok(value) => value.to_response(),
            Err(error) => error.to_response(),
        }
    }
}
