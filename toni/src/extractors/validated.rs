//! Validated extractor wrapper

use validator::Validate;

use super::FromRequest;
use crate::http_helpers::HttpRequest;

/// Wrapper that validates the inner extractor
///
/// # Example
///
/// ```rust,ignore
/// use validator::Validate;
///
/// #[derive(Deserialize, Validate)]
/// struct CreateUserDto {
///     #[validate(length(min = 3))]
///     name: String,
///     #[validate(email)]
///     email: String,
/// }
///
/// #[post("/users")]
/// fn create_user(&self, Json(dto): Validated<Json<CreateUserDto>>) -> String {
///     // dto is already validated here
///     format!("Created user: {}", dto.name)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Validated<T>(pub T);

impl<T> Validated<T> {
    /// Extract the inner value
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Validated<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Validated<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Error type for validation
#[derive(Debug)]
pub enum ValidationError {
    /// Extraction failed
    ExtractionError(String),
    /// Validation failed
    ValidationFailed(validator::ValidationErrors),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::ExtractionError(msg) => write!(f, "Extraction failed: {}", msg),
            ValidationError::ValidationFailed(errors) => {
                write!(f, "Validation failed: {}", errors)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Trait for extractors that contain validatable data
pub trait ValidatableExtractor {
    type Inner: Validate;

    fn get_inner(&self) -> &Self::Inner;
}

// Implement for Json<T> where T: Validate
impl<T: Validate> ValidatableExtractor for super::Json<T> {
    type Inner = T;

    fn get_inner(&self) -> &Self::Inner {
        &self.0
    }
}

// Implement for Path<T> where T: Validate
impl<T: Validate> ValidatableExtractor for super::Path<T> {
    type Inner = T;

    fn get_inner(&self) -> &Self::Inner {
        &self.0
    }
}

// Implement for Query<T> where T: Validate
impl<T: Validate> ValidatableExtractor for super::Query<T> {
    type Inner = T;

    fn get_inner(&self) -> &Self::Inner {
        &self.0
    }
}

// Implement for Body<T> where T: Validate
impl<T: Validate> ValidatableExtractor for super::body::Body<T> {
    type Inner = T;

    fn get_inner(&self) -> &Self::Inner {
        &self.0
    }
}

impl<E> FromRequest for Validated<E>
where
    E: FromRequest + ValidatableExtractor + Send,
    E::Error: std::fmt::Display + Send + Sync + 'static,
{
    type Error = ValidationError;

    async fn from_request(req: HttpRequest) -> Result<Self, Self::Error> {
        let extracted = E::from_request(req)
            .await
            .map_err(|e| ValidationError::ExtractionError(e.to_string()))?;

        extracted
            .get_inner()
            .validate()
            .map_err(ValidationError::ValidationFailed)?;

        Ok(Validated(extracted))
    }
}
