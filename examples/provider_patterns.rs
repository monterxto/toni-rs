//! Provider pattern reference showing all macro variants and token types.
//!
//! This demonstrates every valid combination of provider macros with different token types.
//! For runtime tests, see `integration-tests/tests/di_providers.rs`.

use std::time::Duration;
use toni::di::APP_GUARD;
use toni::{injectable, module, provider_alias, provider_factory, provider_token, provider_value};

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

#[module(
    providers: [
        ConfigService,
        LoggerService,

        // provider_value! - Static constants with different token types
        provider_value!("APP_NAME", "ToniApp".to_string()),
        provider_value!("PORT", 3000_u16),
        provider_value!(Duration, Duration::from_secs(60)),
        provider_value!(APP_GUARD, "global_guard".to_string()),

        // provider_factory! - Sync and async factories
        provider_factory!("REQUEST_ID", || {
            format!("req_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis())
        }),
        provider_factory!("APP_INFO", |config: ConfigService| {
            format!("App running in {} mode", config.get_env())
        }),
        // Async factory auto-detected by 'async' keyword
        provider_factory!("ASYNC_STATUS", async |logger: LoggerService| {
            tokio::time::sleep(Duration::from_millis(1)).await;
            logger.log("System initialized")
        }),

        // provider_alias! - Create alternate tokens pointing to existing providers
        provider_alias!("Config", ConfigService),
        provider_alias!("APP_PORT", "PORT"),

        // provider_token! - Register type under custom token (type NOT auto-registered)
        provider_token!("PRIMARY_CONFIG", ConfigService),
    ],
    exports: [],
)]
impl ProviderPatternsModule {}

fn main() {
    println!("Provider patterns example compiled successfully!");
    println!("\nDemonstrated patterns:");
    println!("  - provider_value! with string/type/const tokens");
    println!("  - provider_factory! with sync/async, with/without deps");
    println!("  - provider_alias! for alternate tokens");
    println!("  - provider_token! for custom token registration");
}
