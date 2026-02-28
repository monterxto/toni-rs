//! Framework-agnostic WebSocket helpers
//!
//! Provides utility functions that can be reused across different WebSocket implementations.

use std::collections::HashMap;

use super::{WsClient, WsHandshake};

/// Create a WsClient from HTTP headers (framework-agnostic)
///
/// Generates a unique client ID and extracts handshake information from headers.
///
/// # Arguments
/// * `headers` - Map of header names to values from the HTTP upgrade request
///
/// # Returns
/// A `WsClient` instance with unique ID and handshake information
///
/// # Example
/// ```ignore
/// let headers = extract_headers_from_request(&http_request);
/// let client = create_client_from_headers(headers);
/// gateway.handle_connect(client).await?;
/// ```
pub fn create_client_from_headers(headers: HashMap<String, String>) -> WsClient {
    WsClient {
        id: uuid::Uuid::new_v4().to_string(),
        handshake: WsHandshake {
            headers,
            query: HashMap::new(), // TODO: Extract from URL if needed
            remote_addr: None,     // TODO: Extract from connection if available
        },
        extensions: Default::default(),
    }
}
