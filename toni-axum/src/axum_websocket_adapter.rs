use axum::{extract::ws::Message, http::HeaderMap};
use std::collections::HashMap;

use toni::websocket::{WsError, WsMessage};

pub(crate) fn axum_to_ws_message(msg: Message) -> Result<WsMessage, WsError> {
    match msg {
        Message::Text(text) => Ok(WsMessage::Text(text.to_string())),
        Message::Binary(data) => Ok(WsMessage::Binary(data.to_vec())),
        Message::Ping(data) => Ok(WsMessage::Ping(data.to_vec())),
        Message::Pong(data) => Ok(WsMessage::Pong(data.to_vec())),
        Message::Close(_) => Err(WsError::ConnectionClosed("Close frame received".into())),
    }
}

pub(crate) fn ws_message_to_axum(msg: WsMessage) -> Result<Message, WsError> {
    match msg {
        WsMessage::Text(text) => Ok(Message::Text(text.into())),
        WsMessage::Binary(data) => Ok(Message::Binary(data.into())),
        WsMessage::Ping(data) => Ok(Message::Ping(data.into())),
        WsMessage::Pong(data) => Ok(Message::Pong(data.into())),
        WsMessage::Close => Ok(Message::Close(None)),
    }
}

/// Extract HTTP upgrade headers into a framework-agnostic map.
pub fn extract_headers(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}
