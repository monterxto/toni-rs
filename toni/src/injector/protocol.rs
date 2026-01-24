//! Protocol abstraction for execution context
//!
//! Enables guards, interceptors, and error handlers to work across HTTP, WebSocket,
//! and RPC protocols through a unified context interface.

use crate::http_helpers::{HttpRequest, HttpResponse};
use crate::websocket::{WsClient, WsMessage};

/// Protocol-specific data for execution context
#[derive(Debug)]
pub enum Protocol {
    Http {
        request: HttpRequest,
        response: Option<HttpResponse>,
    },

    WebSocket {
        client: WsClient,
        message: WsMessage,
        event: String,
    },

    // Future: Rpc { data, context }
}

/// Protocol type identifier for runtime detection in guards and interceptors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolType {
    Http,
    WebSocket,
    // Future: Rpc
}

impl Protocol {
    pub fn http(request: HttpRequest) -> Self {
        Self::Http {
            request,
            response: None,
        }
    }

    pub fn websocket(client: WsClient, message: WsMessage, event: impl Into<String>) -> Self {
        Self::WebSocket {
            client,
            message,
            event: event.into(),
        }
    }

    pub fn protocol_type(&self) -> ProtocolType {
        match self {
            Protocol::Http { .. } => ProtocolType::Http,
            Protocol::WebSocket { .. } => ProtocolType::WebSocket,
        }
    }
}
