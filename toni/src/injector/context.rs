use std::sync::Arc;

use crate::{
    http_helpers::{HttpRequest, HttpResponse, RequestBody, RequestPart, RouteMetadata},
    rpc::{RpcContext as RpcCallContext, RpcData, RpcError},
    traits_helpers::validate::Validatable,
    websocket::{WsClient, WsMessage},
};

use super::{
    Protocol, ProtocolType,
    protocol_context::{
        HttpContext, HttpContextMut, RpcContext, RpcContextMut, WsContext, WsContextMut,
    },
};

/// Execution context for all protocols
///
/// Unified interface for guards, interceptors, and error handlers to work across
/// HTTP, WebSocket, and RPC with protocol switching.
#[derive(Debug)]
pub struct Context {
    protocol: Protocol,
    route_metadata: Option<Arc<RouteMetadata>>, // None for global handlers (404, error filters)
    should_abort: bool,
    dto: Option<Box<dyn Validatable>>,
}

impl Context {
    pub fn new(req: HttpRequest, route_metadata: Arc<RouteMetadata>) -> Self {
        Self {
            protocol: Protocol::http(req),
            route_metadata: Some(route_metadata),
            should_abort: false,
            dto: None,
        }
    }

    pub fn from_request(req: impl Into<HttpRequest>) -> Self {
        let req = req.into();
        Self {
            protocol: Protocol::http(req),
            route_metadata: Some(Arc::new(RouteMetadata::new())),
            should_abort: false,
            dto: None,
        }
    }

    pub fn from_parts(parts: RequestPart) -> Self {
        Self::from_request(HttpRequest::from_parts(parts, RequestBody::empty()))
    }

    pub fn from_websocket(
        client: WsClient,
        message: WsMessage,
        event: impl Into<String>,
        route_metadata: Option<Arc<RouteMetadata>>,
    ) -> Self {
        Self {
            protocol: Protocol::websocket(client, message, event),
            route_metadata,
            should_abort: false,
            dto: None,
        }
    }

    pub fn from_rpc(
        data: RpcData,
        context: RpcCallContext,
        route_metadata: Option<Arc<RouteMetadata>>,
    ) -> Self {
        Self {
            protocol: Protocol::rpc(data, context),
            route_metadata,
            should_abort: false,
            dto: None,
        }
    }

    // Protocol switching

    pub fn protocol_type(&self) -> ProtocolType {
        self.protocol.protocol_type()
    }

    pub fn switch_to_http(&self) -> Option<HttpContext<'_>> {
        match &self.protocol {
            Protocol::Http {
                parts, response, ..
            } => Some(HttpContext { parts, response }),
            _ => None,
        }
    }

    pub fn switch_to_http_mut(&mut self) -> Option<HttpContextMut<'_>> {
        match &mut self.protocol {
            Protocol::Http {
                parts,
                response,
                body,
            } => Some(HttpContextMut {
                parts,
                response,
                body,
            }),
            _ => None,
        }
    }

    pub fn switch_to_ws(&self) -> Option<WsContext<'_>> {
        match &self.protocol {
            Protocol::WebSocket {
                client,
                message,
                event,
                response,
            } => Some(WsContext {
                client,
                message,
                event: event.as_str(),
                response,
            }),
            _ => None,
        }
    }

    pub fn switch_to_ws_mut(&mut self) -> Option<WsContextMut<'_>> {
        match &mut self.protocol {
            Protocol::WebSocket {
                client,
                message,
                event,
                response,
            } => Some(WsContextMut {
                client,
                message,
                event: event.as_str(),
                response,
            }),
            _ => None,
        }
    }

    pub fn switch_to_rpc(&self) -> Option<RpcContext<'_>> {
        match &self.protocol {
            Protocol::Rpc {
                data,
                context,
                response,
            } => Some(RpcContext {
                data,
                call_context: context,
                response,
            }),
            _ => None,
        }
    }

    pub fn switch_to_rpc_mut(&mut self) -> Option<RpcContextMut<'_>> {
        match &mut self.protocol {
            Protocol::Rpc {
                data,
                context,
                response,
            } => Some(RpcContextMut {
                data,
                call_context: context,
                response,
            }),
            _ => None,
        }
    }

    // Consuming extractors

    /// Consume the context and extract the HTTP response.
    ///
    /// # Panics
    /// Panics if not an HTTP context or response was not set.
    pub fn into_http_response(self) -> HttpResponse {
        match self.protocol {
            Protocol::Http { response, .. } => response.expect("Response not set in context"),
            _ => panic!("Expected HTTP context"),
        }
    }

    /// Consume the context and extract the WebSocket response.
    ///
    /// # Panics
    /// Panics if not a WebSocket context or response was not set.
    pub fn into_ws_response(self) -> Result<Option<WsMessage>, crate::websocket::WsError> {
        match self.protocol {
            Protocol::WebSocket { response, .. } => response.unwrap_or_else(|| {
                Err(crate::websocket::WsError::Internal(
                    "Response not set in context".into(),
                ))
            }),
            _ => panic!("Expected WebSocket context"),
        }
    }

    // Metadata

    pub fn metadata(&self) -> Option<&RouteMetadata> {
        self.route_metadata.as_deref()
    }

    // Universal

    /// Short-circuits execution (guards use this to prevent handler from running)
    pub fn abort(&mut self) {
        self.should_abort = true;
    }

    pub fn should_abort(&self) -> bool {
        self.should_abort
    }

    pub fn set_dto(&mut self, dto: Box<dyn Validatable>) {
        self.dto = Some(dto);
    }

    pub fn get_dto(&self) -> Option<&dyn Validatable> {
        self.dto.as_deref()
    }
}
