//! # toni-axum
//!
//! Axum adapter for the Toni framework.
//!
//! This crate provides an implementation of Toni's `HttpAdapter` and `WebSocketAdapter` traits
//! for the Axum web framework.
//!
//! ## Usage
//!
//! ```ignore
//! use toni_axum::AxumAdapter;
//!
//! #[tokio::main]
//! async fn main() {
//!     let adapter = AxumAdapter::new();
//!     let mut app = ToniFactory::create(AppModule::module_definition(), adapter).await;
//!     app.listen(3000, "127.0.0.1").await;
//! }
//! ```

mod axum_adapter;
mod axum_websocket_adapter;
pub(crate) mod tokio_sender;

pub use axum_adapter::AxumAdapter;
pub use tokio_sender::TokioSender;

pub use toni::{HttpAdapter, WebSocketAdapter};
