//! Test for Static Method Support in Controllers
//!
//! Verifies that:
//! 1. Controllers can have methods without `&self` (static/associated functions)
//! 2. Static methods work with both singleton and request scopes
//! 3. Static methods can coexist with instance methods
//! 4. Static methods work with extractors

use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody, HttpRequest};
use toni_axum::AxumAdapter;
use toni_config::{Config, ConfigModule};

#[derive(Config, Clone)]
struct StaticTestConfig {
    #[env("TEST_VALUE")]
    #[default("test".to_string())]
    pub value: String,
}

// ============================================================================
// TEST 1: Pure Static Method Controller (no dependencies)
// ============================================================================

#[controller("/static", pub struct StaticController {})]
impl StaticController {
    #[get("/hello")]
    fn hello(_req: HttpRequest) -> ToniBody {
        ToniBody::Text("Hello from static method".to_string())
    }

    #[get("/world")]
    fn world(_req: HttpRequest) -> ToniBody {
        ToniBody::Text("World from static method".to_string())
    }
}

#[module(
    imports: [ConfigModule::<StaticTestConfig>::from_env().unwrap()],
    controllers: [StaticController],
    providers: [],
)]
impl StaticTestModule {}

#[tokio::test]
#[serial]
async fn test_static_method_controller() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29090;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(StaticTestModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test first static method
            let response = client
                .get(format!("http://127.0.0.1:{}/static/hello", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Hello from static method");
            println!("Static hello: {}", body);

            // Test second static method
            let response = client
                .get(format!("http://127.0.0.1:{}/static/world", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "World from static method");
            println!("Static world: {}", body);

            println!("Test passed: Static methods work correctly");
        })
        .await;
}

// ============================================================================
// TEST 2: Mixed Static and Instance Methods
// ============================================================================

#[injectable(pub struct MixedService {})]
impl MixedService {
    pub fn get_instance_message(&self) -> String {
        "From instance method".to_string()
    }
}

#[controller("/mixed", pub struct MixedController { #[inject]service: MixedService })]
impl MixedController {
    // Instance method - uses self.service
    #[get("/instance")]
    fn instance_method(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.service.get_instance_message())
    }

    // Static method - no self
    #[get("/static")]
    fn static_method(_req: HttpRequest) -> ToniBody {
        ToniBody::Text("From static method".to_string())
    }
}

#[module(
    imports: [ConfigModule::<StaticTestConfig>::from_env().unwrap()],
    controllers: [MixedController],
    providers: [MixedService],
)]
impl MixedTestModule {}

#[tokio::test]
#[serial]
async fn test_mixed_static_and_instance_methods() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29091;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(MixedTestModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test instance method
            let response = client
                .get(format!("http://127.0.0.1:{}/mixed/instance", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "From instance method");
            println!("Instance method: {}", body);

            // Test static method
            let response = client
                .get(format!("http://127.0.0.1:{}/mixed/static", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "From static method");
            println!("Static method: {}", body);

            println!("Test passed: Mixed static and instance methods work correctly");
        })
        .await;
}

// ============================================================================
// TEST 3: Request-scoped Controller with Static Methods
// ============================================================================

#[controller("/request-static", scope = "request", pub struct RequestScopedStaticController {})]
impl RequestScopedStaticController {
    #[get("/test")]
    fn test(_req: HttpRequest) -> ToniBody {
        ToniBody::Text("Static method in request-scoped controller".to_string())
    }
}

#[module(
    imports: [ConfigModule::<StaticTestConfig>::from_env().unwrap()],
    controllers: [RequestScopedStaticController],
    providers: [],
)]
impl RequestScopedStaticTestModule {}

#[tokio::test]
#[serial]
async fn test_request_scoped_static_methods() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29092;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(RequestScopedStaticTestModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test static method in request-scoped controller
            let response = client
                .get(format!("http://127.0.0.1:{}/request-static/test", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Static method in request-scoped controller");
            println!("Request-scoped static: {}", body);

            println!("Test passed: Request-scoped static methods work correctly");
        })
        .await;
}

// ============================================================================
// TEST 4: Async Static Methods
// ============================================================================

#[controller("/async-static", pub struct AsyncStaticController {})]
impl AsyncStaticController {
    #[get("/test")]
    async fn test(_req: HttpRequest) -> ToniBody {
        // Simulate async work
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        ToniBody::Text("Async static method".to_string())
    }
}

#[module(
    imports: [ConfigModule::<StaticTestConfig>::from_env().unwrap()],
    controllers: [AsyncStaticController],
    providers: [],
)]
impl AsyncStaticTestModule {}

#[tokio::test]
#[serial]
async fn test_async_static_methods() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29093;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(AsyncStaticTestModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test async static method
            let response = client
                .get(format!("http://127.0.0.1:{}/async-static/test", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Async static method");
            println!("Async static: {}", body);

            println!("Test passed: Async static methods work correctly");
        })
        .await;
}
