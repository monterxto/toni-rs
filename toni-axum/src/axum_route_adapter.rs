use std::collections::HashMap;

use anyhow::{anyhow, Result};
use axum::{
    body::to_bytes,
    extract::{Path, Query},
    http::{HeaderMap, HeaderName, HeaderValue, Request, Response, StatusCode},
    RequestPartsExt,
};
use serde_json::Value;
use std::str::FromStr;

use toni::{http_helpers::Extensions, Body, HttpRequest, HttpResponse, IntoResponse, RouteAdapter};

pub struct AxumRouteAdapter;

impl RouteAdapter for AxumRouteAdapter {
    type Request = Request<axum::body::Body>;
    type Response = Response<axum::body::Body>;

    async fn adapt_request(request: Self::Request) -> Result<HttpRequest> {
        let (mut parts, body) = request.into_parts();
        let body_bytes = to_bytes(body, usize::MAX).await?;
        let bytes = body_bytes.to_vec();

        // Check content-type to determine how to parse body
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let body = if content_type.contains("application/octet-stream") {
            // Binary data - keep as bytes
            Body::Binary(bytes)
        } else if let Ok(body_str) = String::from_utf8(bytes) {
            // Try parsing as UTF-8 first
            if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                Body::Json(json)
            } else {
                Body::Text(body_str)
            }
        } else {
            // Not valid UTF-8 and no explicit binary content-type
            // This is likely binary data without proper content-type header
            Body::Binary(body_bytes.to_vec())
        };

        let Path(path_params) = parts
            .extract::<Path<HashMap<String, String>>>()
            .await
            .map_err(|e| anyhow!("Failed to extract path parameters: {:?}", e))?;

        let Query(query_params) = parts
            .extract::<Query<HashMap<String, String>>>()
            .await
            .map_err(|e| anyhow!("Failed to extract query parameters: {:?}", e))?;

        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
            .collect();

        Ok(HttpRequest {
            body,
            headers,
            method: parts.method.to_string(),
            uri: parts.uri.to_string(),
            query_params,
            path_params,
            extensions: Extensions::new(),
        })
    }

    fn adapt_response(
        response: Box<dyn IntoResponse<Response = HttpResponse>>,
    ) -> Result<Self::Response> {
        let response = response.to_response();

        let status =
            StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut body_content_type = "text/plain";

        let body = match response.body {
            Some(Body::Text(text)) => axum::body::Body::from(text),
            Some(Body::Json(json)) => {
                body_content_type = "application/json";
                let vec = serde_json::to_vec(&json)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize JSON: {}", e))?;
                axum::body::Body::from(vec)
            }
            Some(Body::Binary(bytes)) => {
                body_content_type = "application/octet-stream";
                axum::body::Body::from(bytes)
            }
            _ => axum::body::Body::empty(),
        };

        let mut headers = HeaderMap::new();

        let content_type = body_content_type;
        headers.insert(
            HeaderName::from_str("Content-Type")
                .map_err(|e| anyhow::anyhow!("Failed to parse header name Content-Type: {}", e))?,
            HeaderValue::from_static(content_type),
        );

        for (k, v) in &response.headers {
            if let Ok(header_name) = HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(header_value) = HeaderValue::from_str(v) {
                    headers.insert(header_name, header_value);
                }
            }
        }

        let mut res = Response::builder()
            .status(status)
            .body(body)
            .map_err(|e| anyhow::anyhow!("Failed to build response: {}", e))?;

        res.headers_mut().extend(headers);

        Ok(res)
    }
}
