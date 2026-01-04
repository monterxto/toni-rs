use std::fmt::Debug;

use serde_json::Value;

use super::{Body, HttpResponse};

pub trait IntoResponse: Debug {
    type Response;

    fn to_response(&self) -> Self::Response;
}

impl IntoResponse for HttpResponse {
    type Response = Self;

    fn to_response(&self) -> Self {
        self.clone()
    }
}

impl IntoResponse for Body {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(self.clone()),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for u16 {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            status: *self,
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for Vec<(String, String)> {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            headers: self.clone(),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for (u16, Body) {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(self.1.clone()),
            status: self.0,
            ..HttpResponse::new()
        }
    }
}

// impl<T1, T2> IntoResponse for (T1, T2)
// where
//     T1: IntoResponse<Response = HttpResponse>,
//     T2: IntoResponse<Response = HttpResponse>,
// {
//     type Response = HttpResponse;

//     fn to_response(&self) -> HttpResponse {
//         let mut response = self.0.to_response();
//         let part = self.1.to_response();

//         response.status = part.status;
//         response.headers.extend(part.headers);
//         response.body = part.body;

//         response
//     }
// }

impl IntoResponse for Value {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(Body::Json(self.clone())),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for String {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(Body::Text(self.clone())),
            ..HttpResponse::new()
        }
    }
}

impl IntoResponse for &'static str {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        HttpResponse {
            body: Some(Body::Text(self.to_string())),
            ..HttpResponse::new()
        }
    }
}

// Support for Result<T, E> where both T and E implement IntoResponse
impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse<Response = HttpResponse>,
    E: IntoResponse<Response = HttpResponse>,
{
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        match self {
            Ok(value) => value.to_response(),
            Err(error) => error.to_response(),
        }
    }
}
