use std::sync::Arc;

use crate::{
    async_trait,
    http_helpers::{HttpMethod, HttpRequest, HttpResponse, RouteMetadata},
    middleware::{Middleware, MiddlewareChain},
    structs_helpers::EnhancerMetadata,
    traits_helpers::{Controller, ErrorHandler, Guard, Interceptor, InterceptorNext, Pipe},
};

use super::Context;

/// Represents the next step in the interceptor chain
struct ChainNext {
    interceptors: Vec<Arc<dyn Interceptor>>,
    instance: Arc<Box<dyn Controller>>,
    pipes: Vec<Arc<dyn Pipe>>,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
    route_metadata: Arc<RouteMetadata>,
}

#[async_trait]
impl InterceptorNext for ChainNext {
    async fn run(self: Box<Self>, context: &mut Context) {
        InstanceWrapper::execute_with_interceptors(
            context,
            &self.interceptors,
            &self.instance,
            &self.pipes,
            &self.error_handlers,
            &self.route_metadata,
        )
        .await;
    }
}

pub struct InstanceWrapper {
    instance: Arc<Box<dyn Controller>>,
    guards: Vec<Arc<dyn Guard>>,
    interceptors: Vec<Arc<dyn Interceptor>>,
    pipes: Vec<Arc<dyn Pipe>>,
    middleware_chain: MiddlewareChain,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
    route_metadata: Arc<RouteMetadata>,
}

impl InstanceWrapper {
    pub fn new(
        instance: Arc<Box<dyn Controller>>,
        enhancer_metadata: EnhancerMetadata,
        global_enhancers: EnhancerMetadata,
    ) -> Self {
        // Merge enhancers: global first, then controller/method
        // Execution order: global < controller < method
        let mut guards = global_enhancers.guards;
        guards.extend(enhancer_metadata.guards);

        let mut interceptors = global_enhancers.interceptors;
        interceptors.extend(enhancer_metadata.interceptors);

        let mut pipes = global_enhancers.pipes;
        pipes.extend(enhancer_metadata.pipes);

        let mut error_handlers = global_enhancers.error_handlers;
        error_handlers.extend(enhancer_metadata.error_handlers);

        let route_metadata = instance.get_route_metadata();

        Self {
            instance,
            guards,
            interceptors,
            pipes,
            middleware_chain: MiddlewareChain::new(),
            error_handlers,
            route_metadata,
        }
    }

    pub fn get_path(&self) -> String {
        self.instance.get_path()
    }

    pub fn get_method(&self) -> HttpMethod {
        self.instance.get_method()
    }

    pub fn add_middleware(&mut self, middleware: Arc<dyn Middleware>) {
        self.middleware_chain.use_middleware(middleware);
    }

    pub fn set_middleware(&mut self, middleware: Vec<Arc<dyn Middleware>>) {
        for m in middleware {
            self.middleware_chain.use_middleware(m);
        }
    }

    /// Get the controller instance for lifecycle hook checks
    pub fn get_instance(&self) -> Arc<Box<dyn Controller>> {
        self.instance.clone()
    }

    pub async fn handle_request(&self, req: HttpRequest) -> HttpResponse {
        let instance = self.instance.clone();
        let guards = self.guards.clone();
        let interceptors = self.interceptors.clone();
        let pipes = self.pipes.clone();
        let error_handlers_for_controller = self.error_handlers.clone();
        let error_handlers_for_middleware = self.error_handlers.clone();
        let route_metadata = self.route_metadata.clone();

        // Store request reference for middleware error handling
        // We need to clone here because middleware.execute takes ownership
        let req_clone = req.clone();

        // Execute middleware chain with controller as the final handler
        let middleware_result = self
            .middleware_chain
            .execute(req, move |req| {
                let instance = instance.clone();
                let guards = guards.clone();
                let interceptors = interceptors.clone();
                let pipes = pipes.clone();
                let error_handlers = error_handlers_for_controller.clone();
                let route_metadata = route_metadata.clone();

                Box::pin(async move {
                    Self::execute_controller_logic(
                        req,
                        instance,
                        guards,
                        interceptors,
                        pipes,
                        error_handlers,
                        route_metadata,
                    )
                    .await
                })
            })
            .await;

        // Handle the result from middleware chain
        match middleware_result {
            Ok(response) => response,
            Err(e) => {
                // HttpError carries an intended HTTP status — use it directly
                // rather than collapsing to 500.
                if let Some(http_err) = e.downcast_ref::<crate::errors::HttpError>() {
                    return http_err.to_response();
                }

                let error_msg = e.to_string();
                for handler in error_handlers_for_middleware.iter().rev() {
                    let error: Box<dyn std::error::Error + Send + Sync> = Box::new(
                        std::io::Error::new(std::io::ErrorKind::Other, error_msg.clone()),
                    );
                    if let Some(response) = handler.handle_error(error, &req_clone).await {
                        return response;
                    }
                }

                let mut error_response = HttpResponse::new();
                error_response.status = 500;
                error_response.body = Some(crate::http_helpers::Body::json(serde_json::json!({
                    "error": "Internal Server Error",
                    "message": "An error occurred while processing the request"
                })));
                error_response
            }
        }
    }

    /// Execute the controller logic with guards, interceptors, and pipes
    async fn execute_controller_logic(
        req: HttpRequest,
        instance: Arc<Box<dyn Controller>>,
        guards: Vec<Arc<dyn Guard>>,
        interceptors: Vec<Arc<dyn Interceptor>>,
        pipes: Vec<Arc<dyn Pipe>>,
        error_handlers: Vec<Arc<dyn ErrorHandler>>,
        route_metadata: Arc<RouteMetadata>,
    ) -> HttpResponse {
        let mut context = Context::new(req, route_metadata.clone());

        // Execute guards
        for guard in &guards {
            if !guard.can_activate(&context) {
                // Get the guard's response (or create default 403 if not set)
                let guard_response = if context.get_response_ref().is_some() {
                    std::mem::take(context.get_response_mut())
                } else {
                    let mut forbidden = HttpResponse::new();
                    forbidden.status = 403;
                    forbidden.body = Some(crate::Body::text("Forbidden"));
                    forbidden
                };

                // Extract request from context for error handler
                let request = context.take_request();
                // Route through ErrorHandler if available
                return Self::handle_error_response(guard_response, &error_handlers, request).await;
            }
        }

        // Execute interceptors wrapping the handler
        Self::execute_with_interceptors(
            &mut context,
            &interceptors,
            &instance,
            &pipes,
            &error_handlers,
            &route_metadata,
        )
        .await;

        context.get_response()
    }

    /// Helper: Route error responses (status >= 400) through ErrorHandler
    /// Uses the last handler in the vec (most specific: method > controller > global)
    async fn handle_error_response(
        response: HttpResponse,
        error_handlers: &[Arc<dyn ErrorHandler>],
        request: &HttpRequest,
    ) -> HttpResponse {
        if response.status >= 400 {
            // Reconstruct HttpError from response to preserve type information
            let http_error = Self::response_to_http_error(&response);

            for handler in error_handlers.iter().rev() {
                let error: Box<dyn std::error::Error + Send> = Box::new(http_error.clone());
                if let Some(handled) = handler.handle_error(error, request).await {
                    return handled;
                }
            }
        }
        response
    }

    /// Execute handler wrapped by interceptors (onion/Russian doll pattern)
    async fn execute_with_interceptors(
        context: &mut Context,
        interceptors: &[Arc<dyn Interceptor>],
        instance: &Arc<Box<dyn Controller>>,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
        route_metadata: &Arc<RouteMetadata>,
    ) {
        // If no interceptors, execute handler directly with error handling
        if interceptors.is_empty() {
            Self::execute_handler_with_error_handling(context, instance, pipes, error_handlers)
                .await;
            return;
        }

        // Get first interceptor and remaining
        let (first, rest) = interceptors.split_first().unwrap();

        // Create the "next" handler that wraps the rest of the chain
        let next = ChainNext {
            interceptors: rest.to_vec(),
            instance: instance.clone(),
            pipes: pipes.to_vec(),
            error_handlers: error_handlers.to_vec(),
            route_metadata: route_metadata.clone(),
        };

        // Execute this interceptor with the next chain
        first.intercept(context, Box::new(next)).await;
    }

    /// Execute the actual handler (pipes + controller)
    async fn execute_handler(
        context: &mut Context,
        instance: &Arc<Box<dyn Controller>>,
        pipes: &[Arc<dyn Pipe>],
    ) {
        // Get and validate DTO
        let dto = instance.get_body_dto(context.take_request());
        if let Some(dto) = dto {
            match dto.validate_dto() {
                Ok(()) => {
                    context.set_dto(dto);
                }
                Err(validation_errors) => {
                    let error_body = serde_json::json!({
                        "error": "Validation failed",
                        "details": validation_errors.to_string()
                    });
                    let response = crate::http_helpers::HttpResponse {
                        body: Some(crate::http_helpers::Body::json(error_body)),
                        status: 400,
                        headers: vec![],
                    };
                    context.set_response(response);
                    context.abort();
                    return;
                }
            }
        }

        // Execute pipes
        for pipe in pipes {
            pipe.process(context);
            if context.should_abort() {
                return;
            }
        }

        // Execute controller
        let req = context.take_request().clone();
        let controller_response = instance.execute(req).await;
        context.set_response(controller_response);
    }

    /// Execute handler with error handling support
    async fn execute_handler_with_error_handling(
        context: &mut Context,
        instance: &Arc<Box<dyn Controller>>,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
    ) {
        Self::execute_handler(context, instance, pipes).await;

        if !error_handlers.is_empty() {
            let needs_error_handling = context
                .get_response_ref()
                .map(|r| r.status >= 400)
                .unwrap_or(false);

            if needs_error_handling {
                let http_response = std::mem::take(context.get_response_mut());
                let request = context.take_request();
                let http_error = Self::response_to_http_error(&http_response);

                for handler in error_handlers.iter().rev() {
                    let error: Box<dyn std::error::Error + Send> = Box::new(http_error.clone());
                    if let Some(handled_response) = handler.handle_error(error, request).await {
                        context.set_response(handled_response);
                        return;
                    }
                }

                context.set_response(http_response);
            }
        }
    }

    /// Reconstruct HttpError from HttpResponse
    /// This preserves the error type for proper error handler matching
    fn response_to_http_error(response: &HttpResponse) -> crate::errors::HttpError {
        let message = if let Some(body) = &response.body {
            if let Some(bytes) = body.try_bytes() {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
                    v.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("HTTP Error")
                        .to_string()
                } else {
                    std::str::from_utf8(bytes)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("HTTP {} Error", response.status))
                }
            } else {
                format!("HTTP {} Error", response.status)
            }
        } else {
            format!("HTTP {} Error", response.status)
        };

        match response.status {
            400 => crate::errors::HttpError::bad_request(message),
            401 => crate::errors::HttpError::unauthorized(message),
            403 => crate::errors::HttpError::forbidden(message),
            404 => crate::errors::HttpError::not_found(message),
            409 => crate::errors::HttpError::conflict(message),
            422 => crate::errors::HttpError::unprocessable_entity(message),
            500 => crate::errors::HttpError::internal_server_error(message),
            status => crate::errors::HttpError::custom(status, message),
        }
    }
}
