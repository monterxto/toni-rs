//! Path parameter extractor

use super::FromRequest;
use serde::de::DeserializeOwned;
use std::str::FromStr;

/// Extractor for path parameters
///
/// # Example
///
/// ```rust,ignore
/// #[get("/users/:id")]
/// fn get_user(&self, Path(id): Path<i32>) -> String {
///     format!("User {}", id)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Path<T>(pub T);

impl<T> Path<T> {
    /// Extract the inner value
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Path<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Path<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Error type for path extraction
#[derive(Debug)]
pub enum PathError {
    /// The parameter was not found in the path
    NotFound(String),
    /// Failed to parse the parameter value
    ParseError(String),
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::NotFound(name) => write!(f, "Path parameter '{}' not found", name),
            PathError::ParseError(msg) => write!(f, "Failed to parse path parameter: {}", msg),
        }
    }
}

impl std::error::Error for PathError {}

/// Helper to extract a single path parameter by name
pub fn extract_path_param<T: FromStr>(
    req: &crate::http_helpers::HttpRequest,
    name: &str,
) -> Result<T, PathError>
where
    T::Err: std::fmt::Display,
{
    let value = req
        .path_params
        .get(name)
        .ok_or_else(|| PathError::NotFound(name.to_string()))?;

    value
        .parse::<T>()
        .map_err(|e| PathError::ParseError(format!("{}: {}", name, e)))
}

/// Implement FromRequest for Path<T> where T is deserializable
/// This allows automatic extraction of path parameters from the request
impl<T: DeserializeOwned> FromRequest for Path<T> {
    type Error = PathError;

    fn from_request(req: &crate::http_helpers::HttpRequest) -> Result<Self, Self::Error> {
        // Convert path_params HashMap to a format serde can deserialize
        let json_value = serde_json::to_value(&req.path_params).map_err(|e| {
            PathError::ParseError(format!("Failed to serialize path params: {}", e))
        })?;

        let deserialized: T = serde_json::from_value(json_value).map_err(|e| {
            PathError::ParseError(format!("Failed to deserialize path params: {}", e))
        })?;

        Ok(Path(deserialized))
    }
}
