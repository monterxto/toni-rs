use std::sync::Arc;

use crate::{
    http_helpers::{HttpRequest, HttpResponse, RouteMetadata, ToResponse},
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

    pub fn from_request(req: HttpRequest) -> Self {
        Self {
            protocol: Protocol::http(req),
            route_metadata: Some(Arc::new(RouteMetadata::new())),
            should_abort: false,
            dto: None,
        }
    }

    // Protocol Methods

    pub fn protocol_type(&self) -> ProtocolType {
        self.protocol.protocol_type()
    }

    /// HTTP protocol access (returns None for other protocols)
    pub fn switch_to_http(&self) -> Option<(&HttpRequest, &Option<HttpResponse>)> {
        match &self.protocol {
            Protocol::Http { request, response } => Some((request, response)),
            _ => None,
        }
    }

    pub fn switch_to_http_mut(&mut self) -> Option<(&mut HttpRequest, &mut Option<HttpResponse>)> {
        match &mut self.protocol {
            Protocol::Http { request, response } => Some((request, response)),
            _ => None,
        }
    }

    /// WebSocket protocol access (returns None for other protocols)
    pub fn switch_to_ws(&self) -> Option<(&WsClient, &WsMessage, &str)> {
        match &self.protocol {
            Protocol::WebSocket { client, message, event } => Some((client, message, event.as_str())),
            _ => None,
        }
    }

    pub fn switch_to_ws_mut(&mut self) -> Option<(&mut WsClient, &mut WsMessage, &str)> {
        match &mut self.protocol {
            Protocol::WebSocket { client, message, event } => Some((client, message, event.as_str())),
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

    /// # Panics
    /// Panics if context is not HTTP. Use `switch_to_http()` for type-safe access.
    pub fn take_request(&self) -> &HttpRequest {
        self.switch_to_http().expect("Expected HTTP context").0
    }

    /// # Panics
    /// Panics if context is not HTTP. Use `switch_to_http_mut()` for type-safe access.
    pub fn set_response(&mut self, response: Box<dyn ToResponse<Response = HttpResponse> + Send>) {
        if let Some((_, response_slot)) = self.switch_to_http_mut() {
            *response_slot = Some(response.to_response());
        } else {
            panic!("Expected HTTP context");
        }
    }

    /// # Panics
    /// Panics if context is not HTTP or response not set.
    pub fn get_response(self) -> Box<dyn ToResponse<Response = HttpResponse> + Send> {
        match self.protocol {
            Protocol::Http { response, .. } => {
                if let Some(resp) = response {
                    Box::new(resp)
                } else {
                    panic!("Response not set in context");
                }
            }
            Protocol::WebSocket { .. } => {
                panic!("get_response() only works for HTTP. Use switch_to_ws() for WebSocket.");
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
