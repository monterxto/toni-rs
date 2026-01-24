use crate::http_helpers::Extensions;
use std::collections::HashMap;

/// WebSocket client/connection
///
/// Represents a connected WebSocket client with handshake data and session information.
#[derive(Debug, Clone)]
pub struct WsClient {
    /// Client identifier (connection ID, session ID, etc.)
    pub id: String,

    /// Handshake information
    pub handshake: WsHandshake,

    /// Protocol-specific extensions (for storing auth data, user info, etc.)
    pub extensions: Extensions,
}

/// WebSocket handshake data
#[derive(Debug, Clone)]
pub struct WsHandshake {
    /// Query parameters from handshake URL
    pub query: HashMap<String, String>,

    /// Headers from handshake request
    pub headers: HashMap<String, String>,

    /// Remote address
    pub remote_addr: Option<String>,
}

impl WsClient {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            handshake: WsHandshake {
                query: HashMap::new(),
                headers: HashMap::new(),
                remote_addr: None,
            },
            extensions: Extensions::new(),
        }
    }

    pub fn with_handshake(mut self, handshake: WsHandshake) -> Self {
        self.handshake = handshake;
        self
    }
}
