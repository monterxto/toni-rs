//! Test for owned fields with #[inject] and #[default] attributes
//!
//! This test demonstrates the new capability for providers to have:
//! - DI-injected fields (marked with #[inject])
//! - Owned fields with custom defaults (marked with #[default(...)])
//! - Owned fields with Default trait fallback (no attributes)

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use toni::{injectable, module};
use toni_config::{Config, ConfigModule, ConfigService};

#[derive(Config, Clone)]
struct TestConfig {
    #[default("test_app".to_string())]
    pub app_name: String,
}

// Test 1: Provider with only owned fields (no DI)
#[injectable(
    pub struct StandaloneService {
        #[default(Duration::from_secs(300))]
        cache_ttl: Duration,

        #[default(1024)]
        buffer_size: usize,

        #[default("cache:".to_string())]
        prefix: String,
    }
)]
impl StandaloneService {
    pub fn get_cache_ttl(&self) -> Duration {
        self.cache_ttl
    }

    pub fn get_buffer_size(&self) -> usize {
        self.buffer_size
    }

    pub fn get_prefix(&self) -> String {
        self.prefix.clone()
    }
}

// Test 2: Provider with mixed DI and owned fields
#[injectable(
    pub struct MixedService {
        #[inject]
        config: ConfigService<TestConfig>,

        #[default(Duration::from_secs(600))]
        timeout: Duration,

        #[default(100)]
        max_retries: usize,
    }
)]
impl MixedService {
    pub fn get_app_name(&self) -> String {
        self.config.get().app_name
    }

    pub fn get_timeout(&self) -> Duration {
        self.timeout
    }

    pub fn get_max_retries(&self) -> usize {
        self.max_retries
    }
}

// Test 3: Provider with Default trait fallback
#[derive(Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

#[injectable(
    pub struct CacheService {
        #[inject]
        config: ConfigService<TestConfig>,

        // No #[default], will use Default::default()
        stats: CacheStats,

        #[default("redis://localhost".to_string())]
        connection_string: String,
    }
)]
impl CacheService {
    pub fn get_stats(&self) -> CacheStats {
        self.stats.clone()
    }

    pub fn get_connection_string(&self) -> String {
        self.connection_string.clone()
    }
}

// Test 4: Provider with interior mutability
use std::sync::Arc;

#[derive(Clone)]
pub struct Counter {
    value: Arc<AtomicU64>,
}

impl Counter {
    pub fn new(initial: u64) -> Self {
        Self {
            value: Arc::new(AtomicU64::new(initial)),
        }
    }

    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

#[injectable(
    pub struct MetricsService {
        #[inject]
        config: ConfigService<TestConfig>,

        #[default(Counter::new(0))]
        request_count: Counter,

        #[default(Counter::new(0))]
        error_count: Counter,
    }
)]
impl MetricsService {
    pub fn increment_requests(&self) {
        self.request_count.increment();
    }

    pub fn increment_errors(&self) {
        self.error_count.increment();
    }

    pub fn get_request_count(&self) -> u64 {
        self.request_count.get()
    }

    pub fn get_error_count(&self) -> u64 {
        self.error_count.get()
    }
}

// Test 5: Complex default expressions
#[injectable(
    pub struct ComplexService {
        #[default({
            let mut v = Vec::new();
            v.push(1);
            v.push(2);
            v.push(3);
            v
        })]
        default_values: Vec<i32>,

        #[default(format!("service_{}", "v1"))]
        service_version: String,
    }
)]
impl ComplexService {
    pub fn get_default_values(&self) -> Vec<i32> {
        self.default_values.clone()
    }

    pub fn get_service_version(&self) -> String {
        self.service_version.clone()
    }
}

// Module definition
#[module(
    imports: [ConfigModule::<TestConfig>::new()],
    providers: [
        StandaloneService,
        MixedService,
        CacheService,
        MetricsService,
        ComplexService
    ],
)]
impl TestModule {}

#[test]
fn test_owned_fields_compile() {
    // This test verifies that the macro generates valid code
    // Actual runtime testing would require instantiating the module
    println!("Owned fields test compiles successfully!");
}

#[tokio::test]
async fn test_owned_fields_runtime() {
    use toni::toni_factory::ToniFactory;
    use toni_axum::AxumAdapter;

    // This would test actual runtime behavior
    // For now, just ensure it compiles
    let _adapter = AxumAdapter::new("127.0.0.1", 0);
    let _factory = ToniFactory::new();

    // TODO: Create the app and verify field values
    // let mut app = ToniFactory::create(TestModule::module_definition(), adapter).await;
    // ... test that services have correct default values ...
}
