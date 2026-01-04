//! HTTP error types that automatically convert to HTTP responses
//!
//! This module provides error types that work with Result<T, E> for
//! clean error handling in your controllers and services.
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```rust
//! use toni::errors::HttpError;
//!
//! fn find_user(id: &str) -> Result<User, HttpError> {
//!     let user = database.find(id)
//!         .ok_or_else(|| HttpError::not_found(format!("User {} not found", id)))?;
//!     Ok(user)
//! }
//! ```
//!
//! ## In Controllers
//!
//! ```rust
//! use toni::{controller, get, errors::HttpError, Body};
//!
//! #[controller("/users", pub struct UserController {})]
//! impl UserController {
//!     #[get("/:id")]
//!     fn get_user(&self, Path(id): Path<String>) -> Result<Body, HttpError> {
//!         let user = self.service.find_user(&id)?;  // Propagates HttpError
//!         Ok(Body::Json(serde_json::to_value(user)?))
//!     }
//! }
//! ```

use serde_json::json;
use std::fmt;

use crate::http_helpers::{Body, HttpResponse, IntoResponse};

/// HTTP error types that map to standard HTTP status codes
///
/// Each variant represents a common HTTP error scenario and automatically
/// converts to an appropriate HTTP response when returned from handlers.
#[derive(Debug, Clone)]
pub enum HttpError {
    /// 400 Bad Request - Client sent invalid data
    BadRequest(String),

    /// 401 Unauthorized - Authentication required or failed
    Unauthorized(String),

    /// 403 Forbidden - Client doesn't have permission
    Forbidden(String),

    /// 404 Not Found - Resource doesn't exist
    NotFound(String),

    /// 409 Conflict - Request conflicts with current state
    Conflict(String),

    /// 422 Unprocessable Entity - Validation failed
    UnprocessableEntity(String),

    /// 500 Internal Server Error - Server-side error
    InternalServerError(String),

    /// Custom error with any status code
    Custom { status: u16, message: String },
}

impl HttpError {
    /// Create a 400 Bad Request error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::bad_request("Invalid input format");
    /// ```
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// Create a 401 Unauthorized error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::unauthorized("Authentication required");
    /// ```
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    /// Create a 403 Forbidden error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::forbidden("Insufficient permissions");
    /// ```
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// Create a 404 Not Found error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::not_found("User not found");
    /// ```
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// Create a 409 Conflict error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::conflict("Email already exists");
    /// ```
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// Create a 422 Unprocessable Entity error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::unprocessable_entity("Validation failed");
    /// ```
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::UnprocessableEntity(message.into())
    }

    /// Create a 500 Internal Server Error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::internal_server_error("Database connection failed");
    /// ```
    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self::InternalServerError(message.into())
    }

    /// Create a custom error with any status code
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::errors::HttpError;
    ///
    /// let error = HttpError::custom(418, "I'm a teapot");
    /// ```
    pub fn custom(status: u16, message: impl Into<String>) -> Self {
        Self::Custom {
            status,
            message: message.into(),
        }
    }

    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest(_) => 400,
            Self::Unauthorized(_) => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::UnprocessableEntity(_) => 422,
            Self::InternalServerError(_) => 500,
            Self::Custom { status, .. } => *status,
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(msg)
            | Self::Unauthorized(msg)
            | Self::Forbidden(msg)
            | Self::NotFound(msg)
            | Self::Conflict(msg)
            | Self::UnprocessableEntity(msg)
            | Self::InternalServerError(msg) => msg,
            Self::Custom { message, .. } => message,
        }
    }

    /// Get the error name/type
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "Bad Request",
            Self::Unauthorized(_) => "Unauthorized",
            Self::Forbidden(_) => "Forbidden",
            Self::NotFound(_) => "Not Found",
            Self::Conflict(_) => "Conflict",
            Self::UnprocessableEntity(_) => "Unprocessable Entity",
            Self::InternalServerError(_) => "Internal Server Error",
            Self::Custom { .. } => "Error",
        }
    }

    /// Convert the error to an HttpResponse
    ///
    /// Creates a JSON response with the following structure:
    /// ```json
    /// {
    ///   "statusCode": 404,
    ///   "message": "User not found",
    ///   "error": "Not Found"
    /// }
    /// ```
    pub fn to_response(&self) -> HttpResponse {
        HttpResponse {
            status: self.status_code(),
            body: Some(Body::Json(json!({
                "statusCode": self.status_code(),
                "message": self.message(),
                "error": self.error_type(),
            }))),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error_type(), self.message())
    }
}

impl std::error::Error for HttpError {}

// Implement IntoResponse for HttpError to enable automatic conversion in handlers
impl IntoResponse for HttpError {
    type Response = HttpResponse;

    fn to_response(&self) -> Self::Response {
        self.to_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error() {
        let error = HttpError::not_found("User not found");
        assert_eq!(error.status_code(), 404);
        assert_eq!(error.message(), "User not found");
        assert_eq!(error.error_type(), "Not Found");
    }

    #[test]
    fn test_bad_request_error() {
        let error = HttpError::bad_request("Invalid input");
        assert_eq!(error.status_code(), 400);
        assert_eq!(error.message(), "Invalid input");
    }

    #[test]
    fn test_custom_error() {
        let error = HttpError::custom(418, "I'm a teapot");
        assert_eq!(error.status_code(), 418);
        assert_eq!(error.message(), "I'm a teapot");
    }

    #[test]
    fn test_to_response() {
        let error = HttpError::not_found("Resource not found");
        let response = error.to_response();

        assert_eq!(response.status, 404);
        assert!(matches!(response.body, Some(Body::Json(_))));
    }

    #[test]
    fn test_display() {
        let error = HttpError::unauthorized("Token expired");
        assert_eq!(format!("{}", error), "Unauthorized: Token expired");
    }
}
