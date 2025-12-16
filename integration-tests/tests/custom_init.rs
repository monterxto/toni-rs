//! Test for custom init method with `init = "method_name"` attribute
//!
//! This demonstrates using a custom constructor method instead of struct literals

use std::time::Duration;
use toni::{injectable, module, HttpAdapter};
use toni_config::{Config, ConfigModule, ConfigService};

#[derive(Config, Clone)]
struct AppConfig {
    #[default("test_app".to_string())]
    pub app_name: String,
}

// Test 1: Custom init with injected dependency
#[injectable(
    init = "new",
    pub struct CacheService {
        #[inject]
        config: ConfigService<AppConfig>,

        prefix: String,
        max_size: usize,
    }
)]
impl CacheService {
    fn new(config: ConfigService<AppConfig>) -> Self {
        let app_name = config.get().app_name;
        Self {
            config,
            prefix: format!("{}:cache:", app_name),
            max_size: 1024,
        }
    }

    pub fn get_prefix(&self) -> String {
        self.prefix.clone()
    }

    pub fn get_max_size(&self) -> usize {
        self.max_size
    }
}

// Test 2: Custom init with no dependencies - ALL fields owned
#[injectable(
    init = "create",
    pub struct StandaloneService {
        value: String,
        count: usize,
    }
)]
impl StandaloneService {
    fn create() -> Self {
        Self {
            value: "computed_value".to_string(),
            count: 42,
        }
    }

    pub fn get_value(&self) -> String {
        self.value.clone()
    }

    pub fn get_count(&self) -> usize {
        self.count
    }
}

// Test 3: Custom init with complex initialization logic
#[injectable(
    init = "build",
    pub struct ComplexService {
        #[inject]
        config: ConfigService<AppConfig>,

        settings: Vec<String>,
        timeout: Duration,
    }
)]
impl ComplexService {
    fn build(config: ConfigService<AppConfig>) -> Self {
        // Complex initialization logic
        let mut settings = Vec::new();
        settings.push("setting1".to_string());
        settings.push("setting2".to_string());
        settings.push("setting3".to_string());

        let timeout = Duration::from_secs(calculate_timeout());

        Self {
            config,
            settings,
            timeout,
        }
    }

    pub fn get_settings(&self) -> Vec<String> {
        self.settings.clone()
    }

    pub fn get_timeout(&self) -> Duration {
        self.timeout
    }
}

fn calculate_timeout() -> u64 {
    // Some complex calculation
    60
}

// Module definition
#[module(
    imports: [ConfigModule::<AppConfig>::new()],
    providers: [CacheService, StandaloneService, ComplexService],
)]
impl TestModule {}

#[test]
fn test_custom_init_compiles() {
    // This test verifies that the macro generates valid code
    println!("Custom init test compiles successfully!");
}

#[tokio::test]
async fn test_custom_init_runtime() {
    use toni::toni_factory::ToniFactory;
    use toni_axum::AxumAdapter;

    // This would test actual runtime behavior
    let _adapter = AxumAdapter::new();
    let _factory = ToniFactory::new();

    // TODO: create the app and verify the init methods were called
    // let app = ToniFactory::create(TestModule::module_definition(), adapter).await;
    // ... test that services were initialized with custom logic ...
}
