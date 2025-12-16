//! Test for Controller Scoping (Singleton vs Request)
//!
//! Verifies that:
//! 1. Singleton controllers (default) are created once at startup
//! 2. Request-scoped controllers are created per HTTP request
//! 3. Dependencies are resolved correctly for each scope

use serial_test::serial;
use toni::{
    controller, controller_struct, get, injectable, module, Body as ToniBody, HttpAdapter,
    HttpRequest,
};
use toni_axum::AxumAdapter;
use toni_config::{Config, ConfigModule};

#[derive(Config, Clone)]
struct ScopeTestConfig {
    #[env("TEST_VALUE")]
    #[default("test".to_string())]
    pub value: String,
}

// Singleton service
#[injectable(pub struct AppService {})]
impl AppService {
    pub fn get_message(&self) -> String {
        "Hello from AppService".to_string()
    }
}

// ============================================================================
// TEST 1: Singleton Controller (Default)
// ============================================================================

#[controller_struct(pub struct SingletonController { #[inject]service: AppService })]
#[controller("/singleton")]
impl SingletonController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.service.get_message())
    }
}

#[module(
    imports: [ConfigModule::<ScopeTestConfig>::from_env().unwrap()],
    controllers: [SingletonController],
    providers: [AppService],
)]
impl SingletonTestModule {}

#[tokio::test]
#[serial]
async fn test_singleton_controller_default() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 28090;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let app = ToniFactory::create(SingletonTestModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Make multiple requests - should all hit the same controller instance
            for i in 1..=3 {
                let response = client
                    .get(format!("http://127.0.0.1:{}/singleton/test", port))
                    .send()
                    .await
                    .expect("Failed to make request");

                assert_eq!(response.status(), 200);
                let body = response.text().await.unwrap();
                assert_eq!(body, "Hello from AppService");
                println!("Request {}: {}", i, body);
            }

            println!("✅ Singleton controller test passed - all requests handled by same instance");
        })
        .await;
}

// ============================================================================
// TEST 2: Explicit Request-scoped Controller
// ============================================================================

#[controller_struct(scope = "request", pub struct RequestController { #[inject]service: AppService })]
#[controller("/request")]
impl RequestController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.service.get_message())
    }
}

#[module(
    imports: [ConfigModule::<ScopeTestConfig>::from_env().unwrap()],
    controllers: [RequestController],
    providers: [AppService],
)]
impl RequestTestModule {}

#[tokio::test]
#[serial]
async fn test_request_scoped_controller_explicit() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 28091;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let app = ToniFactory::create(RequestTestModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Make multiple requests - new controller instance per request
            for i in 1..=3 {
                let response = client
                    .get(format!("http://127.0.0.1:{}/request/test", port))
                    .send()
                    .await
                    .expect("Failed to make request");

                assert_eq!(response.status(), 200);
                let body = response.text().await.unwrap();
                assert_eq!(body, "Hello from AppService");
                println!("Request {}: {}", i, body);
            }

            println!("✅ Request-scoped controller test passed - new instance per request");
        })
        .await;
}

// ============================================================================
// TEST 3: Mixed Scopes - Both Controllers in Same Module
// ============================================================================

#[controller_struct(pub struct BothSingletonController { #[inject]service: AppService })]
#[controller("/both/singleton")]
impl BothSingletonController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(format!("Singleton: {}", self.service.get_message()))
    }
}

#[controller_struct(scope = "request", pub struct BothRequestController { #[inject]service: AppService })]
#[controller("/both/request")]
impl BothRequestController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(format!("Request: {}", self.service.get_message()))
    }
}

#[module(
    imports: [ConfigModule::<ScopeTestConfig>::from_env().unwrap()],
    controllers: [BothSingletonController, BothRequestController],
    providers: [AppService],
)]
impl MixedScopesModule {}

#[tokio::test]
#[serial]
async fn test_mixed_controller_scopes() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 28092;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let app = ToniFactory::create(MixedScopesModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test singleton controller
            let response = client
                .get(format!("http://127.0.0.1:{}/both/singleton/test", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Singleton: Hello from AppService");
            println!("Singleton controller: {}", body);

            // Test request-scoped controller
            let response = client
                .get(format!("http://127.0.0.1:{}/both/request/test", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Request: Hello from AppService");
            println!("Request controller: {}", body);

            println!("✅ Mixed scopes test passed - both controller types work together");
        })
        .await;
}
