use std::sync::Arc;

use crate::{
    async_trait,
    http_helpers::{HttpMethod, HttpRequest, HttpResponse, IntoResponse},
    middleware::{Middleware, MiddlewareChain},
    structs_helpers::EnhancerMetadata,
    traits_helpers::{ControllerTrait, ErrorHandler, Guard, Interceptor, InterceptorNext, Pipe},
};

use super::Context;

/// Represents the next step in the interceptor chain
struct ChainNext {
    interceptors: Vec<Arc<dyn Interceptor>>,
    instance: Arc<Box<dyn ControllerTrait>>,
    pipes: Vec<Arc<dyn Pipe>>,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
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
        )
        .await;
    }
}

pub struct InstanceWrapper {
    instance: Arc<Box<dyn ControllerTrait>>,
    guards: Vec<Arc<dyn Guard>>,
    interceptors: Vec<Arc<dyn Interceptor>>,
    pipes: Vec<Arc<dyn Pipe>>,
    middleware_chain: MiddlewareChain,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
}

impl InstanceWrapper {
    pub fn new(
        instance: Arc<Box<dyn ControllerTrait>>,
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

        Self {
            instance,
            guards,
            interceptors,
            pipes,
            middleware_chain: MiddlewareChain::new(),
            error_handlers,
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

    pub async fn handle_request(
        &self,
        req: HttpRequest,
    ) -> Box<dyn IntoResponse<Response = HttpResponse> + Send> {
        let instance = self.instance.clone();
        let guards = self.guards.clone();
        let interceptors = self.interceptors.clone();
        let pipes = self.pipes.clone();
        let error_handlers_for_controller = self.error_handlers.clone();
        let error_handlers_for_middleware = self.error_handlers.clone();

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

                Box::pin(async move {
                    Self::execute_controller_logic(
                        req,
                        instance,
                        guards,
                        interceptors,
                        pipes,
                        error_handlers,
                    )
                    .await
                })
            })
            .await;

        // Handle the result from middleware chain
        match middleware_result {
            Ok(response) => Box::new(response),
            Err(e) => {
                // Use custom error handler if available (use last handler - most specific)
                if let Some(handler) = error_handlers_for_middleware.last() {
                    let response = handler.handle_error(e, &req_clone).await;
                    Box::new(response)
                } else {
                    // Fallback to default error handling
                    let mut error_response = HttpResponse::new();
                    error_response.status = 500;
                    error_response.body =
                        Some(crate::http_helpers::Body::Json(serde_json::json!({
                            "error": "Internal Server Error",
                            "message": "An error occurred while processing the request"
                        })));
                    Box::new(error_response)
                }
            }
        }
    }

    /// Execute the controller logic with guards, interceptors, and pipes
    async fn execute_controller_logic(
        req: HttpRequest,
        instance: Arc<Box<dyn ControllerTrait>>,
        guards: Vec<Arc<dyn Guard>>,
        interceptors: Vec<Arc<dyn Interceptor>>,
        pipes: Vec<Arc<dyn Pipe>>,
        error_handlers: Vec<Arc<dyn ErrorHandler>>,
    ) -> HttpResponse {
        let mut context = Context::from_request(req);

        // Execute guards
        for guard in &guards {
            if !guard.can_activate(&context) {
                // Get the guard's response (or create default 403 if not set)
                let guard_response = if let Some(response_ref) = context.get_response_ref() {
                    response_ref.to_response()
                } else {
                    let mut forbidden = HttpResponse::new();
                    forbidden.status = 403;
                    forbidden.body = Some(crate::Body::Text("Forbidden".to_string()));
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
        )
        .await;

        context.get_response().to_response()
    }

    /// Helper: Route error responses (status >= 400) through ErrorHandler
    /// Uses the last handler in the vec (most specific: method > controller > global)
    async fn handle_error_response(
        response: HttpResponse,
        error_handlers: &[Arc<dyn ErrorHandler>],
        request: &HttpRequest,
    ) -> HttpResponse {
        // If status >= 400 AND error handler exists, route through it
        if response.status >= 400 {
            if let Some(handler) = error_handlers.last() {
                // Extract error message from response
                let error_msg =
                    if let Some(crate::http_helpers::Body::Json(ref body)) = response.body {
                        body.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("HTTP Error")
                            .to_string()
                    } else if let Some(crate::http_helpers::Body::Text(ref text)) = response.body {
                        text.clone()
                    } else {
                        format!("HTTP {} Error", response.status)
                    };

                // Create error object
                let error: Box<dyn std::error::Error + Send> =
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, error_msg));

                // Let ErrorHandler format the response (use most specific handler)
                return handler.handle_error(error, request).await;
            }
        }
        response
    }

    /// Execute handler wrapped by interceptors (onion/Russian doll pattern)
    async fn execute_with_interceptors(
        context: &mut Context,
        interceptors: &[Arc<dyn Interceptor>],
        instance: &Arc<Box<dyn ControllerTrait>>,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
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
        };

        // Execute this interceptor with the next chain
        first.intercept(context, Box::new(next)).await;
    }

    /// Execute the actual handler (pipes + controller)
    async fn execute_handler(
        context: &mut Context,
        instance: &Arc<Box<dyn ControllerTrait>>,
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
                        body: Some(crate::http_helpers::Body::Json(error_body)),
                        status: 400,
                        headers: vec![],
                    };
                    context.set_response(Box::new(response));
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
        instance: &Arc<Box<dyn ControllerTrait>>,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
    ) {
        // Execute the handler normally
        Self::execute_handler(context, instance, pipes).await;

        // If error handler is available, check if response is an error
        if let Some(handler) = error_handlers.last() {
            // Get the response to inspect it
            let response_box = context.get_response_ref();

            if let Some(response_ref) = response_box {
                // Convert to HttpResponse to check status
                let http_response = response_ref.to_response();

                // If status >= 400, it's an error - use error handler
                if http_response.status >= 400 {
                    let error_msg = if let Some(crate::http_helpers::Body::Json(ref body)) =
                        http_response.body
                    {
                        body.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("HTTP Error")
                            .to_string()
                    } else {
                        format!("HTTP {} Error", http_response.status)
                    };

                    let error: Box<dyn std::error::Error + Send> =
                        Box::new(std::io::Error::new(std::io::ErrorKind::Other, error_msg));

                    // Extract request from context
                    let request = context.take_request();
                    let handled_response = handler.handle_error(error, request).await;
                    context.set_response(Box::new(handled_response));
                }
            }
        }
    }
}
