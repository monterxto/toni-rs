use std::sync::Arc;

use crate::{
    http_helpers::{HttpRequest, HttpResponse, RequestBody, RequestPart, RouteMetadata},
    rpc::{RpcContext, RpcData, RpcError},
    traits_helpers::validate::Validatable,
    websocket::{WsClient, WsMessage},
};

use super::{Protocol, ProtocolType};

/// Execution context for all protocols
///
/// Unified interface for guards, interceptors, and error handlers to work across
/// HTTP, WebSocket, and RPC with protocol switching.
#[derive(Debug)]
pub struct Context {
    protocol: Protocol,
    route_metadata: Option<Arc<RouteMetadata>>, // None for global handlers (404, error filters)
    /// Abort flag for short-circuiting execution
    should_abort: bool,
    /// Validated DTO
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

    /// Create WebSocket context
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

    /// Create RPC context
    pub fn from_rpc(
        data: RpcData,
        context: RpcContext,
        route_metadata: Option<Arc<RouteMetadata>>,
    ) -> Self {
        Self {
            protocol: Protocol::rpc(data, context),
            route_metadata,
            should_abort: false,
            dto: None,
        }
    }

    // Protocol Methods

    pub fn protocol_type(&self) -> ProtocolType {
        self.protocol.protocol_type()
    }

    /// HTTP protocol access (returns None for other protocols)
    pub fn switch_to_http(&self) -> Option<(&RequestPart, &Option<HttpResponse>)> {
        match &self.protocol {
            Protocol::Http {
                parts, response, ..
            } => Some((parts, response)),
            _ => None,
        }
    }

    pub fn switch_to_http_mut(&mut self) -> Option<(&mut RequestPart, &mut Option<HttpResponse>)> {
        match &mut self.protocol {
            Protocol::Http {
                parts, response, ..
            } => Some((parts, response)),
            _ => None,
        }
    }

    /// WebSocket protocol access (returns None for other protocols)
    pub fn switch_to_ws(&self) -> Option<(&WsClient, &WsMessage, &str)> {
        match &self.protocol {
            Protocol::WebSocket {
                client,
                message,
                event,
                ..
            } => Some((client, message, event.as_str())),
            _ => None,
        }
    }

    pub fn switch_to_ws_mut(&mut self) -> Option<(&mut WsClient, &mut WsMessage, &str)> {
        match &mut self.protocol {
            Protocol::WebSocket {
                client,
                message,
                event,
                ..
            } => Some((client, message, event.as_str())),
            _ => None,
        }
    }

    /// Set WebSocket response
    pub fn set_ws_response(
        &mut self,
        response: Result<Option<WsMessage>, crate::websocket::WsError>,
    ) {
        if let Protocol::WebSocket {
            response: response_slot,
            ..
        } = &mut self.protocol
        {
            *response_slot = Some(response);
        } else {
            panic!("Expected WebSocket context");
        }
    }

    /// Get WebSocket response
    pub fn get_ws_response(&self) -> Option<&Result<Option<WsMessage>, crate::websocket::WsError>> {
        match &self.protocol {
            Protocol::WebSocket { response, .. } => response.as_ref(),
            _ => None,
        }
    }

    /// Take WebSocket response (consumes the context)
    pub fn take_ws_response(self) -> Result<Option<WsMessage>, crate::websocket::WsError> {
        match self.protocol {
            Protocol::WebSocket { response, .. } => response.unwrap_or_else(|| {
                Err(crate::websocket::WsError::Internal(
                    "Response not set in context".into(),
                ))
            }),
            _ => panic!("take_ws_response() only works for WebSocket"),
        }
    }

    /// RPC protocol access (returns None for other protocols)
    pub fn switch_to_rpc(&self) -> Option<(&RpcData, &RpcContext)> {
        match &self.protocol {
            Protocol::Rpc { data, context, .. } => Some((data, context)),
            _ => None,
        }
    }

    pub fn switch_to_rpc_mut(&mut self) -> Option<(&mut RpcData, &mut RpcContext)> {
        match &mut self.protocol {
            Protocol::Rpc { data, context, .. } => Some((data, context)),
            _ => None,
        }
    }

    pub fn set_rpc_response(&mut self, response: Result<Option<RpcData>, RpcError>) {
        if let Protocol::Rpc {
            response: response_slot,
            ..
        } = &mut self.protocol
        {
            *response_slot = Some(response);
        } else {
            panic!("Expected RPC context");
        }
    }

    pub fn get_rpc_response(&self) -> Option<&Result<Option<RpcData>, RpcError>> {
        match &self.protocol {
            Protocol::Rpc { response, .. } => response.as_ref(),
            _ => None,
        }
    }

    // Metadata Methods

    /// Get route metadata
    pub fn metadata(&self) -> Option<&RouteMetadata> {
        self.route_metadata.as_deref()
    }

    #[deprecated(since = "0.1.0", note = "Use `metadata()` instead")]
    pub fn route_metadata(&self) -> &RouteMetadata {
        self.metadata().expect("Route metadata not available")
    }

    // Backward Compatibility Helpers (HTTP-specific)
    // TODO: Remove these once all code migrates to switch_to_http()

    /// Borrow the request metadata (method, URI, headers, extensions) without body.
    ///
    /// # Panics
    /// Panics if context is not HTTP.
    pub fn take_request(&self) -> &RequestPart {
        self.switch_to_http().expect("Expected HTTP context").0
    }

    /// Reconstruct the full `HttpRequest` (metadata + body) and move it out of
    /// the context. The body is taken from the internal Mutex; subsequent calls
    /// return an empty body (body has already been consumed).
    ///
    /// # Panics
    /// Panics if not an HTTP context.
    pub fn take_request_owned(&mut self) -> HttpRequest {
        if let Protocol::Http { parts, body, .. } = &mut self.protocol {
            let b = body.lock().take().unwrap_or_else(RequestBody::empty);
            HttpRequest::from_parts(parts.clone(), b)
        } else {
            panic!("Expected HTTP context");
        }
    }

    /// # Panics
    /// Panics if context is not HTTP. Use `switch_to_http_mut()` for type-safe access.
    pub fn set_response(&mut self, response: HttpResponse) {
        if let Some((_, response_slot)) = self.switch_to_http_mut() {
            *response_slot = Some(response);
        } else {
            panic!("Expected HTTP context");
        }
    }

    /// # Panics
    /// Panics if context is not HTTP or response not set.
    pub fn get_response(self) -> HttpResponse {
        match self.protocol {
            Protocol::Http { response, .. } => response.expect("Response not set in context"),
            Protocol::WebSocket { .. } => {
                panic!("get_response() only works for HTTP. Use switch_to_ws() for WebSocket.");
            }
            Protocol::Rpc { .. } => {
                panic!("get_response() only works for HTTP. Use switch_to_rpc() for RPC.");
            }
        }
    }

    pub fn get_response_ref(&self) -> Option<&HttpResponse> {
        self.switch_to_http()
            .and_then(|(_, response)| response.as_ref())
    }

    /// # Panics
    /// Panics if context is not HTTP or response not set.
    pub fn get_response_mut(&mut self) -> &mut HttpResponse {
        self.switch_to_http_mut()
            .expect("Expected HTTP context")
            .1
            .as_mut()
            .expect("Response not set in context")
    }

    // Universal Methods (work for all protocols)

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
