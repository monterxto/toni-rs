use parking_lot::Mutex;

use crate::{
    http_helpers::{HttpRequest, HttpResponse, RequestBody, RequestPart},
    rpc::{RpcContext as RpcCallContext, RpcData, RpcError},
    websocket::{WsClient, WsError, WsMessage},
};

pub struct HttpContext<'a> {
    pub(super) parts: &'a RequestPart,
    pub(super) response: &'a Option<HttpResponse>,
}

impl<'a> HttpContext<'a> {
    pub fn request(&self) -> &'a RequestPart {
        self.parts
    }

    pub fn response(&self) -> Option<&'a HttpResponse> {
        self.response.as_ref()
    }
}

pub struct HttpContextMut<'a> {
    pub(super) parts: &'a RequestPart,
    pub(super) response: &'a mut Option<HttpResponse>,
    pub(super) body: &'a Mutex<Option<RequestBody>>,
}

impl<'a> HttpContextMut<'a> {
    pub fn request(&self) -> &'a RequestPart {
        self.parts
    }

    pub fn response(&self) -> Option<&HttpResponse> {
        self.response.as_ref()
    }

    pub fn response_mut(&mut self) -> Option<&mut HttpResponse> {
        self.response.as_mut()
    }

    pub fn set_response(&mut self, response: HttpResponse) {
        *self.response = Some(response);
    }

    pub fn take_response(&mut self) -> Option<HttpResponse> {
        self.response.take()
    }

    /// Reconstruct the full `HttpRequest` (parts + body) and consume the body.
    /// Subsequent calls return an empty body.
    pub fn take_request(&mut self) -> HttpRequest {
        let body = self.body.lock().take().unwrap_or_else(RequestBody::empty);
        HttpRequest::from_parts(self.parts.clone(), body)
    }
}

pub struct WsContext<'a> {
    pub(super) client: &'a WsClient,
    pub(super) message: &'a WsMessage,
    pub(super) event: &'a str,
    pub(super) response: &'a Option<Result<Option<WsMessage>, WsError>>,
}

impl<'a> WsContext<'a> {
    pub fn client(&self) -> &'a WsClient {
        self.client
    }

    pub fn message(&self) -> &'a WsMessage {
        self.message
    }

    pub fn event(&self) -> &'a str {
        self.event
    }

    pub fn response(&self) -> Option<&'a Result<Option<WsMessage>, WsError>> {
        self.response.as_ref()
    }
}

pub struct WsContextMut<'a> {
    pub(super) client: &'a mut WsClient,
    pub(super) message: &'a mut WsMessage,
    pub(super) event: &'a str,
    pub(super) response: &'a mut Option<Result<Option<WsMessage>, WsError>>,
}

impl<'a> WsContextMut<'a> {
    pub fn client(&self) -> &WsClient {
        self.client
    }

    pub fn message(&self) -> &WsMessage {
        self.message
    }

    pub fn event(&self) -> &'a str {
        self.event
    }

    pub fn set_response(&mut self, response: Result<Option<WsMessage>, WsError>) {
        *self.response = Some(response);
    }
}

pub struct RpcContext<'a> {
    pub(super) data: &'a RpcData,
    pub(super) call_context: &'a RpcCallContext,
    pub(super) response: &'a Option<Result<Option<RpcData>, RpcError>>,
}

impl<'a> RpcContext<'a> {
    pub fn data(&self) -> &'a RpcData {
        self.data
    }

    pub fn call_context(&self) -> &'a RpcCallContext {
        self.call_context
    }

    pub fn response(&self) -> Option<&'a Result<Option<RpcData>, RpcError>> {
        self.response.as_ref()
    }
}

pub struct RpcContextMut<'a> {
    pub(super) data: &'a mut RpcData,
    pub(super) call_context: &'a mut RpcCallContext,
    pub(super) response: &'a mut Option<Result<Option<RpcData>, RpcError>>,
}

impl<'a> RpcContextMut<'a> {
    pub fn data(&self) -> &RpcData {
        self.data
    }

    pub fn call_context(&self) -> &RpcCallContext {
        self.call_context
    }

    pub fn set_response(&mut self, response: Result<Option<RpcData>, RpcError>) {
        *self.response = Some(response);
    }
}
