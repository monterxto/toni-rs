//! End-to-End test demonstrating ConfigModule with real HTTP server and DI injection
//!
//! This test spins up an actual Toni+Axum server, injects ConfigService into a controller,
//! and makes HTTP requests to verify the configuration is accessible.

use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody, HttpAdapter, HttpRequest};
use toni_axum::AxumAdapter;
use toni_config::{Config, ConfigModule, ConfigService};

// Application configuration
#[derive(Config, Clone)]
struct AppConfig {
    #[env("APP_NAME")]
    #[default("ConfigTestApp".to_string())]
    pub app_name: String,

    #[env("APP_VERSION")]
    #[default("1.0.0".to_string())]
    pub version: String,

    #[env("DATABASE_URL")]
    #[default("sqlite://test.db".to_string())]
    pub database_url: String,

    #[env("MAX_CONNECTIONS")]
    #[default(10u32)]
    pub max_connections: u32,
}

// Service that accesses config via ConfigService
#[injectable(
     pub struct AppService {
        #[inject]
        config: ConfigService<AppConfig>
    }
)]
impl AppService {
    pub fn get_app_info(&self) -> String {
        let cfg: AppConfig = self.config.get();
        format!("{} v{}", cfg.app_name, cfg.version)
    }

    pub fn get_database_info(&self) -> String {
        let cfg: AppConfig = self.config.get();
        format!(
            "DB: {} (max {} connections)",
            cfg.database_url, cfg.max_connections
        )
    }

    fn get_full_config(&self) -> AppConfig {
        let cfg: AppConfig = self.config.get();
        cfg
    }
}

// Controller that uses the service
#[controller(
    "/api",
    pub struct AppController {
        #[inject]
        service: AppService,
    }
)]
impl AppController {
    #[get("/info")]
    fn get_info(&self, _req: HttpRequest) -> ToniBody {
        let info: String = self.service.get_app_info();
        ToniBody::Text(info)
    }

    #[get("/database")]
    fn get_database(&self, _req: HttpRequest) -> ToniBody {
        let db_info: String = self.service.get_database_info();
        ToniBody::Text(db_info)
    }

    #[get("/config")]
    fn get_config(&self, _req: HttpRequest) -> ToniBody {
        let config: AppConfig = self.service.get_full_config();
        let json = serde_json::json!({
            "app_name": config.app_name,
            "version": config.version,
            "database_url": config.database_url,
            "max_connections": config.max_connections,
        });
        ToniBody::Json(json)
    }
}

// Application module
#[module(
    imports: [ConfigModule::<AppConfig>::from_env().unwrap()],  // ✨ Can call methods with args!
    controllers: [AppController],
    providers: [AppService],
)]
impl AppModule {}

#[tokio::test]
#[serial]
async fn test_config_injection_e2e() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    // Set up environment variables
    std::env::set_var("APP_NAME", "E2ETestApp");
    std::env::set_var("APP_VERSION", "2.0.0");
    std::env::set_var("DATABASE_URL", "postgres://localhost/e2e_test");
    std::env::set_var("MAX_CONNECTIONS", "50");

    let port = 28080;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let app = ToniFactory::create(AppModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test 1: Get app info
            let response = client
                .get(format!("http://127.0.0.1:{}/api/info", port))
                .send()
                .await
                .expect("Failed to get app info");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "E2ETestApp v2.0.0");

            // Test 2: Get database info
            let response = client
                .get(format!("http://127.0.0.1:{}/api/database", port))
                .send()
                .await
                .expect("Failed to get database info");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(
                body,
                "DB: postgres://localhost/e2e_test (max 50 connections)"
            );

            // Test 3: Get full config as JSON
            let response = client
                .get(format!("http://127.0.0.1:{}/api/config", port))
                .send()
                .await
                .expect("Failed to get config");

            assert_eq!(response.status(), 200);
            let json: serde_json::Value = response.json().await.expect("Failed to parse JSON");
            assert_eq!(json["app_name"], "E2ETestApp");
            assert_eq!(json["version"], "2.0.0");
            assert_eq!(json["database_url"], "postgres://localhost/e2e_test");
            assert_eq!(json["max_connections"], 50);
        })
        .await;

    // Clean up
    std::env::remove_var("APP_NAME");
    std::env::remove_var("APP_VERSION");
    std::env::remove_var("DATABASE_URL");
    std::env::remove_var("MAX_CONNECTIONS");
}

#[tokio::test]
#[serial]
async fn test_config_with_defaults_e2e() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    // Don't set any env vars - should use defaults
    std::env::remove_var("APP_NAME");
    std::env::remove_var("APP_VERSION");
    std::env::remove_var("DATABASE_URL");
    std::env::remove_var("MAX_CONNECTIONS");

    let port = 28081;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let app = ToniFactory::create(AppModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test with default values
            let response = client
                .get(format!("http://127.0.0.1:{}/api/info", port))
                .send()
                .await
                .expect("Failed to get app info");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "ConfigTestApp v1.0.0"); // Default values

            // Test database info with defaults
            let response = client
                .get(format!("http://127.0.0.1:{}/api/database", port))
                .send()
                .await
                .expect("Failed to get database info");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "DB: sqlite://test.db (max 10 connections)");
        })
        .await;
}
