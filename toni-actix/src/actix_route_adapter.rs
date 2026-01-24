use std::collections::HashMap;

use actix_web::{web::Bytes, HttpRequest as ActixHttpRequest, HttpResponse as ActixHttpResponse};
use anyhow::{anyhow, Result};
use serde_json::Value;

use toni::{http_helpers::Extensions, Body, HttpRequest, HttpResponse, RouteAdapter, ToResponse};

pub struct ActixRouteAdapter;

impl ActixRouteAdapter {
    async fn adapt_actix_request(req: ActixHttpRequest, body: Bytes) -> Result<HttpRequest> {
        // Check content-type to determine how to parse body
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Parse body based on content-type
        let body_vec = body.to_vec();
        let body = if content_type.contains("application/octet-stream")
            || content_type.contains("multipart/form-data")
        {
            // Binary data - keep as bytes
            Body::Binary(body_vec)
        } else if let Ok(body_str) = String::from_utf8(body_vec.clone()) {
            // Try parsing as UTF-8 first
            if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                Body::Json(json)
            } else {
                Body::Text(body_str)
            }
        } else {
            // Not valid UTF-8 and no explicit binary content-type
            // This is likely binary data without proper content-type header
            Body::Binary(body_vec)
        };

        // Extract path parameters
        let path_params: HashMap<String, String> = req
            .match_info()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        // Extract query parameters
        let query_string = req.query_string();
        let query_params: HashMap<String, String> = if query_string.is_empty() {
            HashMap::new()
        } else {
            query_string
                .split('&')
                .filter_map(|pair| {
                    if pair.is_empty() {
                        return None;
                    }
                    let mut parts = pair.split('=');
                    let key = parts.next()?;
                    let value = parts.next().unwrap_or("");
                    Some((key.to_string(), value.to_string()))
                })
                .collect()
        };

        // Extract headers
        let headers: Vec<(String, String)> = req
            .headers()
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
            .collect();

        Ok(HttpRequest {
            body,
            headers,
            method: req.method().to_string(),
            uri: req.uri().to_string(),
            query_params,
            path_params,
            extensions: Extensions::new(),
        })
    }

    fn adapt_actix_response(
        response: Box<dyn ToResponse<Response = HttpResponse>>,
    ) -> Result<ActixHttpResponse> {
        let response = response.to_response();

        let status = actix_web::http::StatusCode::from_u16(response.status)
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);

        let mut actix_response = ActixHttpResponse::build(status);

        // Set body
        let actix_response = match response.body {
            Some(Body::Text(text)) => actix_response.content_type("text/plain").body(text),
            Some(Body::Json(json)) => {
                let json_str = serde_json::to_string(&json)
                    .map_err(|e| anyhow!("Failed to serialize JSON: {}", e))?;
                actix_response
                    .content_type("application/json")
                    .body(json_str)
            }
            Some(Body::Binary(bytes)) => actix_response
                .content_type("application/octet-stream")
                .body(bytes),
            None => actix_response.finish(),
        };

        // Set headers
        let mut actix_response = actix_response;
        for (key, value) in response.headers {
            actix_response.headers_mut().insert(
                actix_web::http::header::HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| anyhow!("Failed to parse header name: {}", e))?,
                actix_web::http::header::HeaderValue::from_str(&value)
                    .map_err(|e| anyhow!("Failed to parse header value: {}", e))?,
            );
        }

        Ok(actix_response)
    }
}

impl RouteAdapter for ActixRouteAdapter {
    type Request = (ActixHttpRequest, Bytes);
    type Response = ActixHttpResponse;

    async fn adapt_request(request: Self::Request) -> Result<HttpRequest> {
        Self::adapt_actix_request(request.0, request.1).await
    }

    fn adapt_response(
        response: Box<dyn ToResponse<Response = HttpResponse>>,
    ) -> Result<Self::Response> {
        Self::adapt_actix_response(response)
    }
}
