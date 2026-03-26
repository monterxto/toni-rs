//! JSON body extractor

use serde::de::DeserializeOwned;

use super::FromRequest;
use crate::http_helpers::HttpRequest;

/// Extractor for JSON request body.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Deserialize)]
/// struct CreateUserDto {
///     name: String,
///     email: String,
/// }
///
/// #[post("/users")]
/// fn create_user(&self, Json(dto): Json<CreateUserDto>) -> String {
///     format!("Created user: {}", dto.name)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub enum JsonError {
    NotJson,
    DeserializeError(String),
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonError::NotJson => write!(f, "Request body is not JSON"),
            JsonError::DeserializeError(msg) => {
                write!(f, "Failed to deserialize JSON body: {}", msg)
            }
        }
    }
}

impl std::error::Error for JsonError {}

impl<T: DeserializeOwned> FromRequest for Json<T> {
    type Error = JsonError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        if req.body.is_empty() {
            return Err(JsonError::NotJson);
        }
        let value: T = serde_json::from_slice(&req.body)
            .map_err(|e| JsonError::DeserializeError(e.to_string()))?;
        Ok(Json(value))
    }
}
