//! # toni-axum
//!
//! Axum adapter for the Toni framework.
//!
//! This crate provides an implementation of Toni's `HttpAdapter` trait for the Axum web framework,
//! allowing you to use Axum as the HTTP server for your Toni applications.
//!
//! ## Usage
//!
//! ```ignore
//! use toni::prelude::*;
//! use toni_axum::AxumAdapter;
//!
//! #[tokio::main]
//! async fn main() {
//!     let adapter = AxumAdapter::new();
//!
//!     let app = ToniFactory::create(AppModule::module_definition(), adapter);
//!     app.listen(3000, "127.0.0.1").await;
//! }
//! ```

mod axum_adapter;
mod axum_route_adapter;

pub use axum_adapter::AxumAdapter;
pub use axum_route_adapter::AxumRouteAdapter;

// Re-export commonly used types from toni
pub use toni::{HttpAdapter, RouteAdapter};
