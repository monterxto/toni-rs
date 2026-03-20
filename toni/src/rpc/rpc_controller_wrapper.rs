use std::sync::Arc;

use async_trait::async_trait;

use crate::http_helpers::RouteMetadata;
use crate::injector::Context;
use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, InterceptorNext, Pipe};

use super::{RpcContext, RpcControllerTrait, RpcData, RpcError};

struct RpcChainNext {
    interceptors: Vec<Arc<dyn Interceptor>>,
    controller: Arc<Box<dyn RpcControllerTrait>>,
    pipes: Vec<Arc<dyn Pipe>>,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
}

#[async_trait]
impl InterceptorNext for RpcChainNext {
    async fn run(self: Box<Self>, context: &mut Context) {
        RpcControllerWrapper::execute_with_interceptors_impl(
            context,
            &self.interceptors,
            &self.controller,
            &self.pipes,
            &self.error_handlers,
        )
        .await;
    }
}

/// Wraps an [`RpcControllerTrait`] with the full guard/interceptor/pipe pipeline.
///
/// Parallel to [`GatewayWrapper`] for WebSocket — the framework constructs one
/// wrapper per discovered RPC controller and routes all incoming messages through it.
///
/// [`GatewayWrapper`]: crate::websocket::GatewayWrapper
pub struct RpcControllerWrapper {
    controller: Arc<Box<dyn RpcControllerTrait>>,
    guards: Vec<Arc<dyn Guard>>,
    interceptors: Vec<Arc<dyn Interceptor>>,
    pipes: Vec<Arc<dyn Pipe>>,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
    route_metadata: Arc<RouteMetadata>,
}

impl RpcControllerWrapper {
    pub fn new(
        controller: Arc<Box<dyn RpcControllerTrait>>,
        guards: Vec<Arc<dyn Guard>>,
        interceptors: Vec<Arc<dyn Interceptor>>,
        pipes: Vec<Arc<dyn Pipe>>,
        error_handlers: Vec<Arc<dyn ErrorHandler>>,
        route_metadata: Arc<RouteMetadata>,
    ) -> Self {
        Self {
            controller,
            guards,
            interceptors,
            pipes,
            error_handlers,
            route_metadata,
        }
    }

    pub fn get_patterns(&self) -> Vec<String> {
        self.controller.get_patterns()
    }

    pub async fn handle_message(
        &self,
        data: RpcData,
        context: RpcContext,
    ) -> Result<Option<RpcData>, RpcError> {
        let mut ctx =
            Context::from_rpc(data, context, Some(self.route_metadata.clone()));

        for guard in &self.guards {
            if !guard.can_activate(&ctx) {
                return Err(RpcError::Forbidden("Guard rejected message".into()));
            }
            if ctx.should_abort() {
                return Err(RpcError::Forbidden("Message aborted by guard".into()));
            }
        }

        self.execute_with_interceptors(&mut ctx).await
    }

    async fn execute_with_interceptors(
        &self,
        context: &mut Context,
    ) -> Result<Option<RpcData>, RpcError> {
        Self::execute_with_interceptors_impl(
            context,
            &self.interceptors,
            &self.controller,
            &self.pipes,
            &self.error_handlers,
        )
        .await;

        if context.should_abort() {
            if let Some(response) = context.get_rpc_response() {
                return response.clone();
            }
            return Err(RpcError::Internal(
                "Request aborted by interceptor without response".into(),
            ));
        }

        if let Some(response) = context.get_rpc_response() {
            response.clone()
        } else {
            Err(RpcError::Internal("Handler did not set response".into()))
        }
    }

    /// Stores the result in context rather than returning it directly.
    async fn execute_with_interceptors_impl(
        context: &mut Context,
        interceptors: &[Arc<dyn Interceptor>],
        controller: &Arc<Box<dyn RpcControllerTrait>>,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
    ) {
        if interceptors.is_empty() {
            Self::execute_handler(context, controller, pipes).await;
            let _ = error_handlers;
            return;
        }

        let (first, rest) = interceptors.split_first().unwrap();

        let next = RpcChainNext {
            interceptors: rest.to_vec(),
            controller: controller.clone(),
            pipes: pipes.to_vec(),
            error_handlers: error_handlers.to_vec(),
        };

        first.intercept(context, Box::new(next)).await;
    }

    async fn execute_handler(
        context: &mut Context,
        controller: &Arc<Box<dyn RpcControllerTrait>>,
        pipes: &[Arc<dyn Pipe>],
    ) {
        for pipe in pipes {
            pipe.process(context);
            if context.should_abort() {
                context.set_rpc_response(Err(RpcError::Internal(
                    "Request aborted by pipe".into(),
                )));
                return;
            }
        }

        let Some((data, rpc_ctx)) = context.switch_to_rpc() else {
            context.set_rpc_response(Err(RpcError::Internal(
                "Expected RPC context".into(),
            )));
            return;
        };

        let result = controller
            .handle_message(data.clone(), rpc_ctx.clone())
            .await;

        context.set_rpc_response(result);
    }
}
