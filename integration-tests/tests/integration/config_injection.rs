
use crate::common::TestServer;
use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody};
use toni_config::{Config, ConfigModule, ConfigService};

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
        self.config.get()
    }
}

#[controller(
    "/api",
    pub struct AppController {
        #[inject]
        service: AppService,
    }
)]
impl AppController {
    #[get("/info")]
    fn get_info(&self) -> ToniBody {
        ToniBody::text(self.service.get_app_info())
    }

    #[get("/database")]
    fn get_database(&self) -> ToniBody {
        ToniBody::text(self.service.get_database_info())
    }

    #[get("/config")]
    fn get_config(&self) -> ToniBody {
        let config = self.service.get_full_config();
        let json = serde_json::json!({
            "app_name": config.app_name,
            "version": config.version,
            "database_url": config.database_url,
            "max_connections": config.max_connections,
        });
        ToniBody::json(json)
    }
}

#[module(
    imports: [ConfigModule::<AppConfig>::from_env().unwrap()],
    controllers: [AppController],
    providers: [AppService],
)]
impl AppModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn config_read_from_env_vars() {
    std::env::set_var("APP_NAME", "E2ETestApp");
    std::env::set_var("APP_VERSION", "2.0.0");
    std::env::set_var("DATABASE_URL", "postgres://localhost/e2e_test");
    std::env::set_var("MAX_CONNECTIONS", "50");

    let server = TestServer::start(AppModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/api/info"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "E2ETestApp v2.0.0");

    let resp = server
        .client()
        .get(server.url("/api/database"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "DB: postgres://localhost/e2e_test (max 50 connections)"
    );

    let resp = server
        .client()
        .get(server.url("/api/config"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["app_name"], "E2ETestApp");
    assert_eq!(json["version"], "2.0.0");
    assert_eq!(json["database_url"], "postgres://localhost/e2e_test");
    assert_eq!(json["max_connections"], 50);

    std::env::remove_var("APP_NAME");
    std::env::remove_var("APP_VERSION");
    std::env::remove_var("DATABASE_URL");
    std::env::remove_var("MAX_CONNECTIONS");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn config_falls_back_to_defaults() {
    std::env::remove_var("APP_NAME");
    std::env::remove_var("APP_VERSION");
    std::env::remove_var("DATABASE_URL");
    std::env::remove_var("MAX_CONNECTIONS");

    let server = TestServer::start(AppModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/api/info"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ConfigTestApp v1.0.0");

    let resp = server
        .client()
        .get(server.url("/api/database"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "DB: sqlite://test.db (max 10 connections)"
    );
}
