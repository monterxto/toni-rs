//! Lifecycle hooks example
//!
//! Demonstrates all 5 provider lifecycle hooks:
//! - #[on_module_init]              — runs after DI resolution, before the app starts
//! - #[on_application_bootstrap]    — runs after all modules init, just before listening
//! - #[on_module_destroy]           — runs first during shutdown (release module resources)
//! - #[before_application_shutdown] — runs second during shutdown (stop accepting work)
//! - #[on_application_shutdown]     — runs last during shutdown (final cleanup)
//!
//! Run with: cargo run --example lifecycle_hooks

use toni::*;
use toni_macros::{injectable, module};

// ============================================================================
// Service with Lifecycle Hooks
// ============================================================================

#[injectable(pub struct DatabaseService {
    name: String,
})]
impl DatabaseService {
    pub fn new() -> Self {
        println!("DatabaseService::new() - Constructor called");
        Self {
            name: "PostgreSQL".to_string(),
        }
    }

    pub async fn query(&self, sql: &str) {
        println!("Executing query: {}", sql);
    }

    #[on_module_init]
    async fn connect(&self) {
        println!(
            "DatabaseService::on_module_init() - Connecting to {}",
            self.name
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("DatabaseService connected!");
    }

    #[on_application_bootstrap]
    async fn on_ready(&self) {
        println!("DatabaseService::on_application_bootstrap() - Ready to serve requests");
    }

    #[before_application_shutdown]
    async fn prepare_shutdown(&self, signal: Option<String>) {
        println!(
            "DatabaseService::before_application_shutdown({:?}) - Stop accepting queries",
            signal
        );
    }

    #[on_module_destroy]
    async fn cleanup(&self) {
        println!("DatabaseService::on_module_destroy() - Closing connections");
    }

    #[on_application_shutdown]
    async fn finalize(&self, signal: Option<String>) {
        println!(
            "DatabaseService::on_application_shutdown({:?}) - Final cleanup",
            signal
        );
    }
}

// ============================================================================
// Service without Lifecycle Hooks (for comparison)
// ============================================================================

#[injectable(pub struct LoggerService;)]
impl LoggerService {
    pub fn new() -> Self {
        println!("LoggerService::new() - Constructor called");
        Self
    }

    pub fn log(&self, message: &str) {
        println!("LOG: {}", message);
    }
}

// ============================================================================
// Service that depends on another service
// ============================================================================

#[injectable(
    pub struct UserService {
        db: DatabaseService,
        logger: LoggerService,
})]
impl UserService {
    pub fn new(db: DatabaseService, logger: LoggerService) -> Self {
        println!("UserService::new() - Constructor called");
        Self { db, logger }
    }

    pub async fn get_user(&self, id: u32) {
        self.logger.log(&format!("Getting user {}", id));
        self.db
            .query(&format!("SELECT * FROM users WHERE id = {}", id))
            .await;
    }

    #[on_module_init]
    async fn warm_cache(&self) {
        println!("UserService::on_module_init() - Warming cache");
    }

    #[on_module_destroy]
    async fn flush_cache(&self) {
        println!("UserService::on_module_destroy() - Flushing cache");
    }
}

// ============================================================================
// Module Setup
// ============================================================================

#[module(providers: [DatabaseService, LoggerService, UserService])]
struct AppModule;

// ============================================================================
// Main - Standalone Context Example
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Lifecycle Hooks Demo ===\n");

    println!("Creating standalone application context...\n");

    let mut ctx = ToniFactory::new()
        .create_application_context_with(AppModule)
        .await;

    println!("\nGetting service from DI container...\n");

    let user_service = ctx.get::<UserService>().await?;

    println!("\nDoing some work...\n");

    user_service.get_user(42).await;
    user_service.get_user(123).await;

    println!("\nCalling close() to trigger shutdown hooks...\n");

    ctx.close().await?;

    println!("\nProgram complete!");
    println!("\nHook execution order:");
    println!("  Startup:");
    println!("    1. on_module_init           (connect, warm caches)");
    println!("    2. on_application_bootstrap (ready to serve)");
    println!("  Shutdown:");
    println!("    3. on_module_destroy        (release module resources)");
    println!("    4. before_application_shutdown (stop accepting work)");
    println!("    5. on_application_shutdown  (final cleanup)");

    Ok(())
}
