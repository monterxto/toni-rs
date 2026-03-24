//! Provider registration patterns: value, factory, alias, token
//!
//! Shows all four provider macros, a consumer that injects from them,
//! and how to retrieve a provider from the DI container directly —
//! no HTTP server required.
//!
//! Run with:  cargo run --example provider_patterns

use std::time::Duration;
use toni::{
    injectable, module, provider_alias, provider_factory, provider_token, provider_value,
    toni_factory::ToniFactory,
};

// ---- providers ---------------------------------------------------------------

#[injectable(pub struct ConfigService {
    env: String,
})]
impl ConfigService {
    pub fn new() -> Self {
        Self {
            env: "production".to_string(),
        }
    }

    pub fn get_env(&self) -> String {
        self.env.clone()
    }
}

#[injectable(pub struct LoggerService {
    level: String,
})]
impl LoggerService {
    pub fn new() -> Self {
        Self {
            level: "info".to_string(),
        }
    }

    pub fn log(&self, msg: &str) -> String {
        format!("[{}] {}", self.level, msg)
    }
}

// ---- consumer ----------------------------------------------------------------
//
// Injects values from each provider macro so the resolved output is observable.

#[injectable(pub struct AppInfo {
    // provider_value! — constant injected under a string token
    #[inject("APP_NAME")]
    name: String,

    #[inject("PORT")]
    port: u16,

    // provider_factory! — built once, value injected under a string token
    #[inject("APP_INFO")]
    info: String,

    // provider_factory! with async — same injection, factory ran async
    #[inject("ASYNC_STATUS")]
    status: String,

    // provider_alias! — "Config" resolves to the same instance as ConfigService
    #[inject("Config")]
    config: ConfigService,

    // provider_token! — ConfigService registered under "PRIMARY_CONFIG"
    // without a separate type-token entry
    #[inject("PRIMARY_CONFIG")]
    primary: ConfigService,
})]
impl AppInfo {
    fn print(&self) {
        println!("  app_name  (provider_value):         {}", self.name);
        println!("  port      (provider_value):         {}", self.port);
        println!("  info      (provider_factory + dep): {}", self.info);
        println!("  status    (provider_factory async): {}", self.status);
        println!("  config    (provider_alias):         env={}", self.config.get_env());
        println!("  primary   (provider_token):         env={}", self.primary.get_env());
    }
}

// ---- module ------------------------------------------------------------------

#[module(
    providers: [
        ConfigService,
        LoggerService,

        // provider_value! — static constants under string or type tokens
        provider_value!("APP_NAME", "ToniApp".to_string()),
        provider_value!("PORT", 3000_u16),
        provider_value!(Duration, Duration::from_secs(60)),

        // provider_factory! — sync factory, no deps
        provider_factory!("REQUEST_ID", || {
            format!("req_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis())
        }),
        // sync factory with an injected dep
        provider_factory!("APP_INFO", |config: ConfigService| {
            format!("App running in {} mode", config.get_env())
        }),
        // async factory — detected by the `async` keyword
        provider_factory!("ASYNC_STATUS", async |logger: LoggerService| {
            tokio::time::sleep(Duration::from_millis(1)).await;
            logger.log("System initialized")
        }),

        // provider_alias! — create an alternate token pointing to an existing provider
        provider_alias!("Config", ConfigService),
        provider_alias!("APP_PORT", "PORT"),

        // provider_token! — register a type under a custom token
        // (does NOT create the default type-token entry)
        provider_token!("PRIMARY_CONFIG", ConfigService),

        AppInfo,
    ],
    exports: [],
)]
impl ProviderPatternsModule {}

// ---- main --------------------------------------------------------------------

#[tokio::main]
async fn main() {
    println!("🔧 toni provider patterns\n");

    let app = ToniFactory::new()
        .create_with(ProviderPatternsModule::module_definition())
        .await;

    let info = app
        .get::<AppInfo>()
        .await
        .expect("AppInfo should resolve — check all token names match");

    println!("Resolved values:");
    info.print();
}
