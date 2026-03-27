use super::FromRequest;
use crate::http_helpers::{HttpRequest, PathParams};
use serde::de::DeserializeOwned;
use std::str::FromStr;

/// Extracts typed path parameters from the URL.
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

#[derive(Debug)]
pub enum PathError {
    NotFound(String),
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

pub fn extract_path_param<T: FromStr>(
    req: &HttpRequest,
    name: &str,
) -> Result<T, PathError>
where
    T::Err: std::fmt::Display,
{
    let params = req.extensions().get::<PathParams>();
    let value = params
        .and_then(|p| p.0.get(name))
        .ok_or_else(|| PathError::NotFound(name.to_string()))?;

    value
        .parse::<T>()
        .map_err(|e| PathError::ParseError(format!("{}: {}", name, e)))
}

impl<T: DeserializeOwned> FromRequest for Path<T> {
    type Error = PathError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let params = req.extensions().get::<PathParams>();
        // Round-trip through serde_json::Value so any T: DeserializeOwned works,
        // including structs with multiple named fields.
        let json_value = match params {
            Some(p) => serde_json::to_value(&p.0)
                .map_err(|e| PathError::ParseError(format!("Failed to serialize path params: {}", e)))?,
            None => serde_json::Value::Object(Default::default()),
        };

        let deserialized: T = serde_json::from_value(json_value)
            .map_err(|e| PathError::ParseError(format!("Failed to deserialize path params: {}", e)))?;

        Ok(Path(deserialized))
    }
}
