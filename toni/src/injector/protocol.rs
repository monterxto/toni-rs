//! Protocol abstraction for execution context
//!
//! Enables guards, interceptors, and error handlers to work across HTTP, WebSocket,
//! and RPC protocols through a unified context interface.

use crate::http_helpers::{HttpRequest, HttpResponse};
use crate::rpc::{RpcContext, RpcData};
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

    Rpc {
        data: RpcData,
        context: RpcContext,
    },
}

/// Protocol type identifier for runtime detection in guards and interceptors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolType {
    Http,
    WebSocket,
    Rpc,
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

    pub fn rpc(data: RpcData, context: RpcContext) -> Self {
        Self::Rpc { data, context }
    }

    pub fn protocol_type(&self) -> ProtocolType {
        match self {
            Protocol::Http { .. } => ProtocolType::Http,
            Protocol::WebSocket { .. } => ProtocolType::WebSocket,
            Protocol::Rpc { .. } => ProtocolType::Rpc,
        }
    }
}
