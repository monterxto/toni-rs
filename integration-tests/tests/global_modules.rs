//! Comprehensive test for global module functionality
//!
//! This test verifies:
//! 1. Modules marked with `global: true` make their exports available everywhere
//! 2. Feature modules can inject global providers WITHOUT importing the global module
//! 3. Real HTTP requests work with global providers
//! 4. Both attribute syntax and builder method work

use serial_test::serial;
use std::time::Duration;
use toni::{controller, get, injectable, module, Body as ToniBody, HttpAdapter, HttpRequest};
use toni_axum::AxumAdapter;
use toni_config::{Config, ConfigModule, ConfigService};

// ============= Test Configuration =============
#[derive(Config, Clone)]
struct GlobalTestConfig {
    #[env("GLOBAL_TEST_VALUE")]
    #[default("global_works".to_string())]
    pub value: String,

    #[env("GLOBAL_TEST_COUNT")]
    #[default(42u32)]
    pub count: u32,
}

// ============= Infrastructure Services (Will be Global) =============

#[injectable]
pub struct LoggerService {
    #[inject]
    config: ConfigService<GlobalTestConfig>,
}

impl LoggerService {
    pub fn log(&self, message: &str) -> String {
        format!("[{}] {}", self.config.get().value, message)
    }

    pub fn get_count(&self) -> u32 {
        self.config.get().count
    }
}

#[injectable]
pub struct DatabaseService {
    #[inject]
    config: ConfigService<GlobalTestConfig>,
}

impl DatabaseService {
    pub fn query(&self, sql: &str) -> String {
        format!("DB[{}] executing: {}", self.config.get().value, sql)
    }
}

// ============= Global Infrastructure Module =============
#[module(
    global: true,  // ✅ Mark as global using attribute
    imports: [ConfigModule::<GlobalTestConfig>::from_env().unwrap()],
    providers: [LoggerService, DatabaseService],
    exports: [LoggerService, DatabaseService],
)]
impl GlobalInfraModule {}

// ============= Feature Module 1: User Module =============
// This module does NOT import GlobalInfraModule but can use its providers

#[injectable]
pub struct UserService {
    #[inject]
    logger: LoggerService, // ✅ Resolves from global registry
    #[inject]
    database: DatabaseService, // ✅ Resolves from global registry
}

impl UserService {
    pub fn get_user(&self, id: u32) -> String {
        let log = self.logger.log(&format!("Getting user {}", id));
        let query = self
            .database
            .query(&format!("SELECT * FROM users WHERE id = {}", id));
        format!("{} | {}", log, query)
    }

    pub fn get_logger_count(&self) -> u32 {
        self.logger.get_count()
    }
}

#[controller(
    "/users",
    pub struct UserController {
        #[inject]
        user_service: UserService,
    }
)]
impl UserController {
    #[get("/{id}")]
    fn get_user(&self, _req: HttpRequest) -> ToniBody {
        let result = self.user_service.get_user(123);
        ToniBody::Text(result)
    }

    #[get("/count")]
    fn get_count(&self, _req: HttpRequest) -> ToniBody {
        let count = self.user_service.get_logger_count();
        ToniBody::Text(count.to_string())
    }
}

#[module(
    // ✅ Note: NOT importing GlobalInfraModule!
    controllers: [UserController],
    providers: [UserService],
    exports: [UserService],
)]
impl UserModule {}

// ============= Feature Module 2: Order Module =============
// This module ALSO doesn't import GlobalInfraModule

#[injectable]
pub struct OrderService {
    #[inject]
    logger: LoggerService, // ✅ Resolves from global
    #[inject]
    database: DatabaseService, // ✅ Resolves from global
    #[inject]
    user_service: UserService, // ✅ Resolves from UserModule import
}

impl OrderService {
    pub fn create_order(&self, user_id: u32, product: &str) -> String {
        let user_log = self.user_service.get_user(user_id);
        let order_log = self.logger.log(&format!("Creating order for {}", product));
        let db_query = self.database.query(&format!(
            "INSERT INTO orders (user_id, product) VALUES ({}, '{}')",
            user_id, product
        ));
        format!("{} | {} | {}", user_log, order_log, db_query)
    }
}

#[controller(
    "/orders",
    pub struct OrderController {
        #[inject]
        order_service: OrderService,
    }
)]
impl OrderController {
    #[get("/create")]
    fn create_order(&self, _req: HttpRequest) -> ToniBody {
        let result = self.order_service.create_order(456, "laptop");
        ToniBody::Text(result)
    }
}

#[module(
    imports: [UserModule::new()],  // Import UserModule for UserService
    controllers: [OrderController],
    providers: [OrderService],
)]
impl OrderModule {}

// ============= Root Application Module =============
#[module(
    imports: [
        GlobalInfraModule::new(),  // ✅ Global module registered once
        UserModule::new(),
        OrderModule::new(),
    ],
)]
impl AppModule {}

// ============= TESTS =============

#[tokio::test]
#[serial]
async fn test_global_module_with_real_http_requests() {
    use toni::toni_factory::ToniFactory;

    // Set environment variables
    std::env::set_var("GLOBAL_TEST_VALUE", "production");
    std::env::set_var("GLOBAL_TEST_COUNT", "999");

    let port = 38080;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let mut app = ToniFactory::create(AppModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests
    local
        .run_until(async move {
            // Wait for server to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test 1: UserController can access global LoggerService and DatabaseService
            let response = client
                .get(format!("http://127.0.0.1:{}/users/123", port))
                .send()
                .await
                .expect("Failed to get user");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            // Should contain log from LoggerService with "production" value
            assert!(body.contains("production"));
            assert!(body.contains("Getting user 123"));
            assert!(body.contains("SELECT * FROM users WHERE id = 123"));

            // Test 2: Verify config value from global provider
            let response = client
                .get(format!("http://127.0.0.1:{}/users/count", port))
                .send()
                .await
                .expect("Failed to get count");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "999"); // From environment variable

            // Test 3: OrderController also accesses global providers + imported UserService
            let response = client
                .get(format!("http://127.0.0.1:{}/orders/create", port))
                .send()
                .await
                .expect("Failed to create order");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            // Should contain logs from both UserService and OrderService
            assert!(body.contains("production"));
            assert!(body.contains("Getting user 456"));
            assert!(body.contains("Creating order for laptop"));
            assert!(body.contains("INSERT INTO orders"));
        })
        .await;

    // Cleanup
    std::env::remove_var("GLOBAL_TEST_VALUE");
    std::env::remove_var("GLOBAL_TEST_COUNT");
}

// ============= TEST BUILDER METHOD SYNTAX =============

// Infrastructure for builder method test
#[injectable]
pub struct CacheService {}

impl CacheService {
    pub fn get(&self, key: &str) -> String {
        format!("redis::{}", key)
    }
}

impl Default for CacheService {
    fn default() -> Self {
        Self {}
    }
}

#[module(
    providers: [CacheService],
    exports: [CacheService],
)]
impl CacheModule {}

// Feature using cache
#[injectable]
pub struct ProductService {
    #[inject]
    cache: CacheService,
}

impl ProductService {
    pub fn get_product(&self, id: u32) -> String {
        self.cache.get(&format!("product:{}", id))
    }
}

#[controller(
    "/products",
    pub struct ProductController {
        #[inject]
        product_service: ProductService,
    }
)]
impl ProductController {
    #[get("/{id}")]
    fn get_product(&self, _req: HttpRequest) -> ToniBody {
        let result = self.product_service.get_product(789);
        ToniBody::Text(result)
    }
}

#[module(
    // ✅ Note: NOT importing CacheModule! It's global, so we can use it
    controllers: [ProductController],
    providers: [ProductService],
)]
impl ProductModule {}

// Root with builder method
#[module(
    imports: [
        CacheModule::new().global(),  // ✅ Mark as global using builder method
        ProductModule::new(),
    ],
)]
impl BuilderAppModule {}

#[tokio::test]
#[serial]
async fn test_global_via_builder_method() {
    use toni::toni_factory::ToniFactory;

    let port = 38081;
    let local = tokio::task::LocalSet::new();

    // Spawn server
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let mut app = ToniFactory::create(BuilderAppModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run test
    local
        .run_until(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            let response = client
                .get(format!("http://127.0.0.1:{}/products/789", port))
                .send()
                .await
                .expect("Failed to get product");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("redis::product:789"));
        })
        .await;
}
