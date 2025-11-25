use std::sync::Arc;

use crate::{
    async_trait,
    http_helpers::{HttpMethod, HttpRequest, HttpResponse, IntoResponse},
    middleware::{Middleware, MiddlewareChain},
    structs_helpers::EnhancerMetadata,
    traits_helpers::{ControllerTrait, Guard, Interceptor, InterceptorNext, Pipe},
};

use super::Context;

/// Represents the next step in the interceptor chain
struct ChainNext {
    interceptors: Vec<Arc<dyn Interceptor>>,
    instance: Arc<Box<dyn ControllerTrait>>,
    pipes: Vec<Arc<dyn Pipe>>,
}

#[async_trait]
impl InterceptorNext for ChainNext {
    async fn run(self: Box<Self>, context: &mut Context) {
        InstanceWrapper::execute_with_interceptors(
            context,
            &self.interceptors,
            &self.instance,
            &self.pipes,
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

        Self {
            instance,
            guards,
            interceptors,
            pipes,
            middleware_chain: MiddlewareChain::new(),
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

        // Execute middleware chain with controller as the final handler
        let middleware_result = self
            .middleware_chain
            .execute(req, move |req| {
                let instance = instance.clone();
                let guards = guards.clone();
                let interceptors = interceptors.clone();
                let pipes = pipes.clone();

                Box::pin(async move {
                    Self::execute_controller_logic(req, instance, guards, interceptors, pipes).await
                })
            })
            .await;

        // Handle the result from middleware chain
        match middleware_result {
            Ok(response) => Box::new(response),
            Err(e) => {
                // Convert error to HTTP response
                eprintln!("❌ Middleware error: {}", e);
                let mut error_response = HttpResponse::new();
                error_response.status = 500;
                error_response.body = Some(crate::http_helpers::Body::Json(serde_json::json!({
                    "error": "Internal Server Error",
                    "message": "An error occurred while processing the request"
                })));
                Box::new(error_response)
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
    ) -> HttpResponse {
        let mut context = Context::from_request(req);

        // Execute guards
        for guard in &guards {
            if !guard.can_activate(&context) {
                // If guard rejects but hasn't set a response, return default 403 Forbidden
                if context.get_response_ref().is_none() {
                    let mut forbidden = HttpResponse::new();
                    forbidden.status = 403;
                    forbidden.body = Some(crate::Body::Text("Forbidden".to_string()));
                    return forbidden;
                }
                return context.get_response().to_response();
            }
        }

        // Execute interceptors wrapping the handler
        Self::execute_with_interceptors(&mut context, &interceptors, &instance, &pipes).await;

        context.get_response().to_response()
    }

    /// Execute handler wrapped by interceptors (onion/Russian doll pattern)
    async fn execute_with_interceptors(
        context: &mut Context,
        interceptors: &[Arc<dyn Interceptor>],
        instance: &Arc<Box<dyn ControllerTrait>>,
        pipes: &[Arc<dyn Pipe>],
    ) {
        // If no interceptors, execute handler directly
        if interceptors.is_empty() {
            Self::execute_handler(context, instance, pipes).await;
            return;
        }

        // Get first interceptor and remaining
        let (first, rest) = interceptors.split_first().unwrap();

        // Create the "next" handler that wraps the rest of the chain
        let next = ChainNext {
            interceptors: rest.to_vec(),
            instance: instance.clone(),
            pipes: pipes.to_vec(),
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
}
