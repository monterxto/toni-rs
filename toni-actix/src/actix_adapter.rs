use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use actix_web::{
    dev::Server, web, web::Bytes, App, HttpRequest as ActixHttpRequest,
    HttpResponse as ActixHttpResponse, HttpServer,
};
use toni::{
    http_adapter::HttpRequestCallbacks,
    http_helpers::{PathParams, RequestBody},
    HttpAdapter, HttpMethod, HttpRequest, HttpResponse,
};

#[derive(Clone)]
pub struct ActixAdapter {
    routes: Arc<std::sync::Mutex<Vec<RouteConfig>>>,
}

struct RouteConfig {
    path: String,
    method: HttpMethod,
    callbacks: Arc<HttpRequestCallbacks>,
}

impl ActixAdapter {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl Default for ActixAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ActixAdapter {
    async fn adapt_request(request: (ActixHttpRequest, Bytes)) -> Result<HttpRequest> {
        let (req, body) = request;

        let path_params: HashMap<String, String> = req
            .match_info()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let method = req
            .method()
            .as_str()
            .parse::<http::Method>()
            .unwrap_or(http::Method::GET);
        let uri = req
            .uri()
            .to_string()
            .parse::<http::Uri>()
            .unwrap_or_else(|_| http::Uri::default());

        let mut builder = http::Request::builder().method(method).uri(uri);
        for (name, value) in req.headers().iter() {
            if let Ok(val) = http::HeaderValue::from_bytes(value.as_bytes()) {
                if let Ok(key) = http::HeaderName::from_bytes(name.as_str().as_bytes()) {
                    builder = builder.header(key, val);
                }
            }
        }
        let (mut http_parts, _) = builder.body(()).unwrap().into_parts();
        http_parts.extensions.insert(PathParams(path_params));

        Ok(HttpRequest::from_parts(
            http_parts,
            RequestBody::Buffered(web::Bytes::from(body.to_vec())),
        ))
    }

    async fn adapt_response(response: HttpResponse) -> Result<ActixHttpResponse> {
        let status = actix_web::http::StatusCode::from_u16(response.status)
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);

        let mut builder = ActixHttpResponse::build(status);

        let actix_response = match response.body {
            Some(toni_body) => {
                let ct = toni_body
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let bytes = {
                    use http_body_util::BodyExt;
                    toni_body
                        .into_box_body()
                        .collect()
                        .await
                        .map(|c| c.to_bytes())
                        .unwrap_or_default()
                };
                builder.content_type(ct.as_str()).body(bytes.to_vec())
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
}

impl HttpAdapter for ActixAdapter {
    fn bind(&mut self, method: HttpMethod, path: &str, callbacks: Arc<HttpRequestCallbacks>) {
        let mut routes = self.routes.lock().unwrap();
        routes.push(RouteConfig {
            path: path.to_string(),
            method,
            callbacks,
        });
    }

    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>> {
        let addr = format!("{}:{}", hostname, port);
        let routes = self.routes.clone();

        let server: Server = HttpServer::new(move || {
            let mut app = App::new();
            let routes_guard = routes.lock().unwrap();

            for route_config in routes_guard.iter() {
                let callbacks = route_config.callbacks.clone();
                let path = route_config.path.clone();

                macro_rules! actix_route {
                    ($method_fn:expr) => {{
                        app = app.route(
                            &path,
                            $method_fn.to(move |req: ActixHttpRequest, body: Bytes| {
                                let callbacks = callbacks.clone();
                                async move {
                                    let http_req = match Self::adapt_request((req, body)).await {
                                        Ok(r) => r,
                                        Err(_) => {
                                            return ActixHttpResponse::InternalServerError()
                                                .finish()
                                        }
                                    };
                                    let http_res = callbacks.handle(http_req).await;
                                    Self::adapt_response(http_res).await.unwrap_or_else(|_| {
                                        ActixHttpResponse::InternalServerError().finish()
                                    })
                                }
                            }),
                        )
                    }};
                }

                match route_config.method {
                    HttpMethod::GET => actix_route!(web::get()),
                    HttpMethod::POST => actix_route!(web::post()),
                    HttpMethod::PUT => actix_route!(web::put()),
                    HttpMethod::DELETE => actix_route!(web::delete()),
                    HttpMethod::PATCH => actix_route!(web::patch()),
                    HttpMethod::HEAD => actix_route!(web::head()),
                    HttpMethod::OPTIONS => {
                        actix_route!(web::route().method(actix_web::http::Method::OPTIONS))
                    }
                    HttpMethod::TRACE => {
                        actix_route!(web::route().method(actix_web::http::Method::TRACE))
                    }
                    HttpMethod::CONNECT => {
                        actix_route!(web::route().method(actix_web::http::Method::CONNECT))
                    }
                };
            }

            app
        })
        .bind(&addr)
        .with_context(|| format!("Failed to bind to {}", addr))?
        .run();

        Ok(Box::pin(async move {
            if let Err(e) = server
                .await
                .with_context(|| "Actix server encountered an error")
            {
                tracing::error!(error = %e, "Actix server error");
                std::process::exit(1);
            }
        }))
    }
}
