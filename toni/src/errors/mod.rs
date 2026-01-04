//! Error handling types and traits for Toni
//!
//! This module provides error handling that works seamlessly with HTTP responses.
//! Use Result<T, HttpError> in your handlers for automatic error conversion.

pub mod http_error;

pub use http_error::HttpError;
