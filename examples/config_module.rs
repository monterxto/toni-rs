//! Reading typed configuration from environment variables
//!
//! ConfigModule reads env vars at startup, falls back to declared defaults,
//! and makes the config available anywhere in the DI graph via ConfigService<T>.
//!
//! Run with:  cargo run --example config_module
//!
//! Override defaults:
//!   APP_NAME="MyApp" APP_PORT=9000 cargo run --example config_module
//!
//! Test:
//!   curl http://127.0.0.1:3000/config

use serde_json::json;
use toni::toni_factory::ToniFactory;
use toni::{controller, get, injectable, module, Body};
use toni_axum::AxumAdapter;
use toni_config::{Config, ConfigModule, ConfigService};

#[derive(Config, Clone)]
struct AppConfig {
    #[env("APP_NAME")]
    #[default("toni-app".to_string())]
    pub name: String,

    #[env("APP_PORT")]
    #[default(3000u16)]
    pub port: u16,

    #[env("LOG_LEVEL")]
    #[default("info".to_string())]
    pub log_level: String,

    #[env("MAX_CONNECTIONS")]
    #[default(10u32)]
    pub max_connections: u32,
}

#[injectable(pub struct AppService {
    #[inject]
    config: ConfigService<AppConfig>,
})]
impl AppService {
    pub fn get_config(&self) -> AppConfig {
        self.config.get()
    }
}

#[controller("/config", pub struct ConfigController {
    #[inject]
    service: AppService,
})]
impl ConfigController {
    #[get("/")]
    fn get_config(&self) -> Body {
        let cfg = self.service.get_config();
        Body::json(json!({
            "name":            cfg.name,
            "port":            cfg.port,
            "log_level":       cfg.log_level,
            "max_connections": cfg.max_connections,
        }))
    }
}

#[module(
    imports: [ConfigModule::<AppConfig>::from_env().unwrap()],
    controllers: [ConfigController],
    providers: [AppService],
)]
impl AppModule {}

#[tokio::main]
async fn main() {
    println!("⚙️  toni config module\n");
    println!("  GET http://127.0.0.1:3000/config");
    println!();
    println!("Override any value with env vars before running:");
    println!("  APP_NAME=MyApp APP_PORT=9000 cargo run --example config_module");
    println!();

    let mut app = ToniFactory::new()
        .create_with(AppModule::module_definition())
        .await;

    app.use_http_adapter(AxumAdapter::new(), 3000, "127.0.0.1")
        .unwrap();

    app.start().await;
}
