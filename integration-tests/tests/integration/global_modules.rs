use crate::common::TestServer;
use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody};
use toni_config::{Config, ConfigModule, ConfigService};

#[derive(Config, Clone)]
struct GlobalTestConfig {
    #[env("GLOBAL_TEST_VALUE")]
    #[default("global_works".to_string())]
    pub value: String,

    #[env("GLOBAL_TEST_COUNT")]
    #[default(42u32)]
    pub count: u32,
}

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

#[module(
    global: true,
    imports: [ConfigModule::<GlobalTestConfig>::from_env().unwrap()],
    providers: [LoggerService, DatabaseService],
    exports: [LoggerService, DatabaseService],
)]
impl GlobalInfraModule {}

#[injectable]
pub struct UserService {
    #[inject]
    logger: LoggerService,
    #[inject]
    database: DatabaseService,
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
    fn get_user(&self) -> ToniBody {
        ToniBody::text(self.user_service.get_user(123))
    }

    #[get("/count")]
    fn get_count(&self) -> ToniBody {
        ToniBody::text(self.user_service.get_logger_count().to_string())
    }
}

#[module(
    controllers: [UserController],
    providers: [UserService],
    exports: [UserService],
)]
impl UserModule {}

#[injectable]
pub struct OrderService {
    #[inject]
    logger: LoggerService,
    #[inject]
    database: DatabaseService,
    #[inject]
    user_service: UserService,
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
    fn create_order(&self) -> ToniBody {
        ToniBody::text(self.order_service.create_order(456, "laptop"))
    }
}

#[module(
    imports: [UserModule::new()],
    controllers: [OrderController],
    providers: [OrderService],
)]
impl OrderModule {}

#[module(
    imports: [
        GlobalInfraModule::new(),
        UserModule::new(),
        OrderModule::new(),
    ],
)]
impl AppModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn global_module_providers_accessible_across_feature_modules() {
    std::env::set_var("GLOBAL_TEST_VALUE", "production");
    std::env::set_var("GLOBAL_TEST_COUNT", "999");

    let server = TestServer::start(AppModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/users/123"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("production"));
    assert!(body.contains("Getting user 123"));
    assert!(body.contains("SELECT * FROM users WHERE id = 123"));

    let resp = server
        .client()
        .get(server.url("/users/count"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "999");

    let resp = server
        .client()
        .get(server.url("/orders/create"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("production"));
    assert!(body.contains("Getting user 456"));
    assert!(body.contains("Creating order for laptop"));
    assert!(body.contains("INSERT INTO orders"));

    std::env::remove_var("GLOBAL_TEST_VALUE");
    std::env::remove_var("GLOBAL_TEST_COUNT");
}

// ---- builder method: .global() ----

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
    fn get_product(&self) -> ToniBody {
        ToniBody::text(self.product_service.get_product(789))
    }
}

#[module(
    controllers: [ProductController],
    providers: [ProductService],
)]
impl ProductModule {}

#[module(
    imports: [
        CacheModule::new().global(),
        ProductModule::new(),
    ],
)]
impl BuilderAppModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn global_module_via_builder_method() {
    let server = TestServer::start(BuilderAppModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/products/789"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "redis::product:789");
}
