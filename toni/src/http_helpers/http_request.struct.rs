use std::collections::HashMap;

use bytes::Bytes;

use super::Extensions;

#[derive(Clone, Debug)]
pub struct HttpRequest {
    /// Raw buffered request body. Content-type is in `headers`.
    pub body: Bytes,
    pub headers: Vec<(String, String)>,
    pub method: String,
    pub uri: String,
    pub query_params: HashMap<String, String>,
    pub path_params: HashMap<String, String>,
    pub extensions: Extensions,
}

impl HttpRequest {
    pub fn headers(&self) -> &Vec<(String, String)> {
        &self.headers
    }

    pub fn headers_mut(&mut self) -> &mut Vec<(String, String)> {
        &mut self.headers
    }

    /// Get a specific header value by name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == name_lower)
            .map(|(_, v)| v.as_str())
    }

    /// Check if a header exists (case-insensitive).
    pub fn has_header(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.headers
            .iter()
            .any(|(k, _)| k.to_lowercase() == name_lower)
    }

    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }
}
