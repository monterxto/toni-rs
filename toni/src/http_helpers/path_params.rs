use std::collections::HashMap;

/// Route path parameters extracted by the adapter after route matching.
///
/// Stored in `http::Extensions` on the request so middleware and extractors
/// can read them without coupling to a specific adapter.
#[derive(Debug, Clone, Default)]
pub struct PathParams(pub HashMap<String, String>);

impl PathParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn into_inner(self) -> HashMap<String, String> {
        self.0
    }
}
