use crate::http_helpers::{HttpRequest, HttpResponse, RequestBody, RequestPart};
use crate::rpc::{RpcContext, RpcData, RpcError};
use crate::websocket::{WsClient, WsMessage};
use parking_lot::Mutex;

/// Protocol-specific data for execution context
#[derive(Debug)]
pub enum Protocol {
    Http {
        // Parts are always present; body is taken once by the handler via
        // direct Protocol::Http destructuring. Storing the body in a Mutex
        // makes Protocol: Sync even though RequestBody may contain a !Sync stream.
        parts: RequestPart,
        body: Mutex<Option<RequestBody>>,
        response: Option<HttpResponse>,
    },

    WebSocket {
        client: WsClient,
        message: WsMessage,
        event: String,
        response: Option<Result<Option<WsMessage>, crate::websocket::WsError>>,
    },

    Rpc {
        data: RpcData,
        context: RpcContext,
        response: Option<Result<Option<RpcData>, RpcError>>,
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
        let (parts, body) = request.into_parts();
        Self::Http {
            parts,
            body: Mutex::new(Some(body)),
            response: None,
        }
    }

    pub fn websocket(client: WsClient, message: WsMessage, event: impl Into<String>) -> Self {
        Self::WebSocket {
            client,
            message,
            event: event.into(),
            response: None,
        }
    }

    pub fn rpc(data: RpcData, context: RpcContext) -> Self {
        Self::Rpc {
            data,
            context,
            response: None,
        }
    }

    pub fn protocol_type(&self) -> ProtocolType {
        match self {
            Protocol::Http { .. } => ProtocolType::Http,
            Protocol::WebSocket { .. } => ProtocolType::WebSocket,
            Protocol::Rpc { .. } => ProtocolType::Rpc,
        }
    }
}
