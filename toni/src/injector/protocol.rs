//! Protocol abstraction for execution context
//!
//! Enables guards, interceptors, and error handlers to work across HTTP, WebSocket,
//! and RPC protocols through a unified context interface.

use crate::http_helpers::{HttpRequest, HttpResponse};

/// Protocol-specific data for execution context
///
/// Future: WebSocket and RPC variants.
#[derive(Debug)]
pub enum Protocol {
    Http {
        request: HttpRequest,
        response: Option<HttpResponse>,
    },
    // Future protocols will be added here:
    // WebSocket {
    //     client: WsClient,
    //     message: WsMessage,
    //     event: String,
    // },
    //
    // Rpc {
    //     data: RpcData,
    //     context: RpcContext,
    // },
}

/// Protocol type identifier for runtime detection in guards and interceptors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolType {
    Http,
    // Future: WebSocket, Rpc
}

impl Protocol {
    pub fn http(request: HttpRequest) -> Self {
        Self::Http {
            request,
            response: None,
        }
    }

    pub fn protocol_type(&self) -> ProtocolType {
        match self {
            Protocol::Http { .. } => ProtocolType::Http,
        }
    }
}
