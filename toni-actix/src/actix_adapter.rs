use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

use actix_web::{
    dev::Server, web, web::Bytes, App, HttpRequest as ActixHttpRequest,
    HttpResponse as ActixHttpResponse, HttpServer,
};
use serde_json::Value;
use toni::{
    http_helpers::Extensions, Body, HttpAdapter, HttpMethod, HttpRequest, HttpResponse,
    InstanceWrapper, ToResponse,
};

#[derive(Clone)]
pub struct ActixAdapter {
    routes: Arc<std::sync::Mutex<Vec<RouteConfig>>>,
}

struct RouteConfig {
    path: String,
    method: HttpMethod,
    handler: Arc<InstanceWrapper>,
}

impl ActixAdapter {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl HttpAdapter for ActixAdapter {
    type Request = (ActixHttpRequest, Bytes);
    type Response = ActixHttpResponse;

    async fn adapt_request(request: Self::Request) -> Result<HttpRequest> {
        let (req, body) = request;

        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let body_vec = body.to_vec();
        let body = if content_type.contains("application/octet-stream")
            || content_type.contains("multipart/form-data")
        {
            Body::Binary(body_vec)
        } else if let Ok(body_str) = String::from_utf8(body_vec.clone()) {
            if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                Body::Json(json)
            } else {
                Body::Text(body_str)
            }
        } else {
            Body::Binary(body_vec)
        };

        let path_params: HashMap<String, String> = req
            .match_info()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

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

    fn adapt_response(
        response: Box<dyn ToResponse<Response = HttpResponse>>,
    ) -> Result<Self::Response> {
        let response = response.to_response();

        let status = actix_web::http::StatusCode::from_u16(response.status)
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);

        let mut builder = ActixHttpResponse::build(status);

        let actix_response = match response.body {
            Some(Body::Text(text)) => builder.content_type("text/plain").body(text),
            Some(Body::Json(json)) => {
                let json_str = serde_json::to_string(&json)
                    .map_err(|e| anyhow!("Failed to serialize JSON: {}", e))?;
                builder.content_type("application/json").body(json_str)
            }
            Some(Body::Binary(bytes)) => {
                builder.content_type("application/octet-stream").body(bytes)
            }
            None => builder.finish(),
        };

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

    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>) {
        let mut routes = self.routes.lock().unwrap();
        routes.push(RouteConfig {
            path: path.to_string(),
            method,
            handler,
        });
    }

    async fn listen(self, port: u16, hostname: &str) -> Result<()> {
        let addr = format!("{}:{}", hostname, port);
        let routes = self.routes.clone();

        println!("Listening on {}", addr);

        let server: Server = HttpServer::new(move || {
            let mut app = App::new();
            let routes_guard = routes.lock().unwrap();

            for route_config in routes_guard.iter() {
                let handler = route_config.handler.clone();
                let path = route_config.path.clone();

                match route_config.method {
                    HttpMethod::GET => {
                        app = app.route(
                            &path,
                            web::get().to(move |req: ActixHttpRequest, body: Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::POST => {
                        app = app.route(
                            &path,
                            web::post().to(move |req: ActixHttpRequest, body: Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::PUT => {
                        app = app.route(
                            &path,
                            web::put().to(move |req: ActixHttpRequest, body: Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::DELETE => {
                        app = app.route(
                            &path,
                            web::delete().to(move |req: ActixHttpRequest, body: Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::PATCH => {
                        app = app.route(
                            &path,
                            web::patch().to(move |req: ActixHttpRequest, body: Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::HEAD => {
                        app = app.route(
                            &path,
                            web::head().to(move |req: ActixHttpRequest, body: Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::OPTIONS => {
                        app = app.route(
                            &path,
                            web::route().method(actix_web::http::Method::OPTIONS).to(
                                move |req: ActixHttpRequest, body: Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixAdapter::handle_request((req, body), handler)
                                            .await
                                            .unwrap()
                                    }
                                },
                            ),
                        );
                    }
                    HttpMethod::TRACE => {
                        app = app.route(
                            &path,
                            web::route().method(actix_web::http::Method::TRACE).to(
                                move |req: ActixHttpRequest, body: Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixAdapter::handle_request((req, body), handler)
                                            .await
                                            .unwrap()
                                    }
                                },
                            ),
                        );
                    }
                    HttpMethod::CONNECT => {
                        app = app.route(
                            &path,
                            web::route().method(actix_web::http::Method::CONNECT).to(
                                move |req: ActixHttpRequest, body: Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixAdapter::handle_request((req, body), handler)
                                            .await
                                            .unwrap()
                                    }
                                },
                            ),
                        );
                    }
                }
            }

            app
        })
        .bind(&addr)
        .with_context(|| format!("Failed to bind to {}", addr))?
        .run();

        server
            .await
            .with_context(|| "Actix server encountered an error")?;

        Ok(())
    }
}
