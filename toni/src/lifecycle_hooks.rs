//! Lifecycle hooks for application startup and shutdown
//!
//! These traits allow providers (services), controllers, and modules to implement
//! initialization and cleanup logic that runs during the application lifecycle.
//!
//! ## Supported Components
//! - `#[injectable]` - Services/Providers
//! - `#[controller]` - HTTP Controllers
//! - `#[module]` - Modules (via `ModuleMetadata` trait methods)
//!
//! ## Hook Order
//! **Startup (automatic):**
//! 1. Dependencies injected
//! 2. `on_module_init()` called
//! 3. `on_application_bootstrap()` called (before app starts listening)
//!
//! **Shutdown (when `app.close()` is called):**
//! 1. `on_module_destroy()` - Module cleanup
//! 2. `before_application_shutdown(signal)` - Stop accepting new work
//! 3. `on_application_shutdown(signal)` - Close connections
//!
//! ## Signal Handling
//! Toni is runtime-agnostic. Users handle signals themselves using their chosen runtime.
//!
//! **Example with Tokio (SIGTERM + SIGINT):**
//! ```ignore
//! use tokio::signal;
//!
//! #[cfg(unix)]
//! async fn shutdown_signal() -> String {
//!     use tokio::signal::unix::{signal, SignalKind};
//!     let mut sigterm = signal(SignalKind::terminate()).unwrap();
//!     let mut sigint = signal(SignalKind::interrupt()).unwrap();
//!
//!     tokio::select! {
//!         _ = sigterm.recv() => "SIGTERM".to_string(),
//!         _ = sigint.recv() => "SIGINT".to_string(),
//!     }
//! }
//!
//! #[cfg(not(unix))]
//! async fn shutdown_signal() -> String {
//!     signal::ctrl_c().await.unwrap();
//!     "SIGINT".to_string()
//! }
//!
//! // In your main function
//! tokio::select! {
//!     _ = app.listen(3000, "0.0.0.0") => {}
//!     signal = shutdown_signal() => {
//!         println!("Received {}", signal);
//!         app.close().await?;
//!     }
//! }
//! ```
//!

use crate::async_trait;

/// Called after all dependencies have been resolved and injected
///
/// Use this to initialize connections, warm caches, or perform startup tasks.
///
/// # Example
/// ```ignore
/// #[async_trait]
/// impl OnModuleInit for DatabaseService {
///     async fn on_module_init(&self) {
///         self.pool.connect().await;
///         println!("Database connected");
///     }
/// }
/// ```
#[async_trait]
pub trait OnModuleInit {
    async fn on_module_init(&self);
}

/// Called after the application is fully initialized but before it starts listening
///
/// This is the last hook before the server begins accepting connections.
/// Use this for final setup tasks that depend on all modules being initialized.
///
/// # Example
/// ```ignore
/// #[async_trait]
/// impl OnApplicationBootstrap for AppService {
///     async fn on_application_bootstrap(&self) {
///         println!("Application is ready to start");
///         self.send_startup_notification().await;
///     }
/// }
/// ```
#[async_trait]
pub trait OnApplicationBootstrap {
    async fn on_application_bootstrap(&self);
}

/// Called when a module is being destroyed during shutdown
///
/// Use this to cleanup resources that belong to a specific module.
///
/// # Example
/// ```ignore
/// #[async_trait]
/// impl OnModuleDestroy for ConnectionManager {
///     async fn on_module_destroy(&self) {
///         self.close_all_connections().await;
///     }
/// }
/// ```
#[async_trait]
pub trait OnModuleDestroy {
    async fn on_module_destroy(&self);
}

/// Called before application shutdown
///
/// Receives the shutdown signal (e.g., "SIGTERM", "SIGINT").
/// Use this to prepare for shutdown (e.g., stop accepting new requests).
///
/// # Example
/// ```ignore
/// #[async_trait]
/// impl BeforeApplicationShutdown for MyService {
///     async fn before_application_shutdown(&self, signal: Option<String>) {
///         println!("Preparing for shutdown: {:?}", signal);
///         self.stop_accepting_work().await;
///     }
/// }
/// ```
#[async_trait]
pub trait BeforeApplicationShutdown {
    async fn before_application_shutdown(&self, signal: Option<String>);
}

/// Called during application shutdown
///
/// Final cleanup step. All resources should be released here.
///
/// # Example
/// ```ignore
/// #[async_trait]
/// impl OnApplicationShutdown for DatabasePool {
///     async fn on_application_shutdown(&self, signal: Option<String>) {
///         self.close_pool().await;
///     }
/// }
/// ```
#[async_trait]
pub trait OnApplicationShutdown {
    async fn on_application_shutdown(&self, signal: Option<String>);
}
