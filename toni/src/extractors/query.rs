use serde::de::DeserializeOwned;

use super::FromRequestParts;
use crate::http_helpers::RequestPart;

/// Extracts typed query parameters from the URL.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Deserialize)]
/// struct SearchParams {
///     q: String,
///     limit: Option<i32>,
/// }
///
/// #[get("/search")]
/// fn search(&self, Query(params): Query<SearchParams>) -> String {
///     format!("Searching for: {}", params.q)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Query<T>(pub T);

impl<T> Query<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Query<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Query<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub enum QueryError {
    DeserializeError(String),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::DeserializeError(msg) => {
                write!(f, "Failed to deserialize query parameters: {}", msg)
            }
        }
    }
}

impl std::error::Error for QueryError {}

impl<T: DeserializeOwned> FromRequestParts for Query<T> {
    type Error = QueryError;

    fn from_request_parts(parts: &RequestPart) -> Result<Self, Self::Error> {
        let query = parts.uri.query().unwrap_or("");
        let value: T = serde_urlencoded::from_str(query)
            .map_err(|e| QueryError::DeserializeError(e.to_string()))?;
        Ok(Query(value))
    }
}
