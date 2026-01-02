use anyhow::{Context, Result};
use std::sync::Arc;

use actix_web::{dev::Server, web, App, HttpServer};
use toni::{HttpAdapter, HttpMethod, InstanceWrapper, RouteAdapter};

use super::ActixRouteAdapter;

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
    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>) {
        println!("Adding route: {} {:?}", path, method);

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
                            web::get().to(move |req: actix_web::HttpRequest, body: web::Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixRouteAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::POST => {
                        app = app.route(
                            &path,
                            web::post().to(move |req: actix_web::HttpRequest, body: web::Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixRouteAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::PUT => {
                        app = app.route(
                            &path,
                            web::put().to(move |req: actix_web::HttpRequest, body: web::Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixRouteAdapter::handle_request((req, body), handler)
                                        .await
                                        .unwrap()
                                }
                            }),
                        );
                    }
                    HttpMethod::DELETE => {
                        app = app.route(
                            &path,
                            web::delete().to(
                                move |req: actix_web::HttpRequest, body: web::Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixRouteAdapter::handle_request((req, body), handler)
                                            .await
                                            .unwrap()
                                    }
                                },
                            ),
                        );
                    }
                    HttpMethod::PATCH => {
                        app = app.route(
                            &path,
                            web::patch().to(
                                move |req: actix_web::HttpRequest, body: web::Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixRouteAdapter::handle_request((req, body), handler)
                                            .await
                                            .unwrap()
                                    }
                                },
                            ),
                        );
                    }
                    HttpMethod::HEAD => {
                        app = app.route(
                            &path,
                            web::head().to(move |req: actix_web::HttpRequest, body: web::Bytes| {
                                let handler = handler.clone();
                                async move {
                                    ActixRouteAdapter::handle_request((req, body), handler)
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
                                move |req: actix_web::HttpRequest, body: web::Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixRouteAdapter::handle_request((req, body), handler)
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
                                move |req: actix_web::HttpRequest, body: web::Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixRouteAdapter::handle_request((req, body), handler)
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
                                move |req: actix_web::HttpRequest, body: web::Bytes| {
                                    let handler = handler.clone();
                                    async move {
                                        ActixRouteAdapter::handle_request((req, body), handler)
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
