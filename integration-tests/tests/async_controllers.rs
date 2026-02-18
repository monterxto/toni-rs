//! Test async controller methods
//!
//! This test verifies that controller methods can be async and properly await async operations

use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody, HttpAdapter, HttpRequest};
use toni_axum::AxumAdapter;

// Simple async service
#[injectable(
    pub struct AsyncService;
)]
impl AsyncService {
    pub async fn fetch_data(&self) -> String {
        // Simulate async operation
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        "async data".to_string()
    }

    pub async fn compute(&self, value: i32) -> i32 {
        // Simulate async computation
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        value * 2
    }
}

// Controller with async methods
#[controller(
    "/async",
    pub struct AsyncController {
        #[inject]
        service: AsyncService,
    }
)]
impl AsyncController {
    // Async method that awaits service call
    #[get("/data")]
    async fn get_data(&self, _req: HttpRequest) -> ToniBody {
        let data = self.service.fetch_data().await;
        ToniBody::Text(data)
    }

    // Async method with some computation
    #[get("/compute")]
    async fn compute(&self, _req: HttpRequest) -> ToniBody {
        let result = self.service.compute(42).await;
        ToniBody::Text(format!("Result: {}", result))
    }

    // Sync method should still work
    #[get("/sync")]
    fn sync_method(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text("sync response".to_string())
    }

    // Async method that does multiple awaits
    #[get("/multi")]
    async fn multi_await(&self, _req: HttpRequest) -> ToniBody {
        let data = self.service.fetch_data().await;
        let result = self.service.compute(10).await;
        ToniBody::Text(format!("{} - {}", data, result))
    }
}

// Application module
#[module(
    controllers: [AsyncController],
    providers: [AsyncService],
)]
impl AsyncModule {}

#[tokio::test]
#[serial]
async fn test_async_controller_methods() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29080;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let mut app = ToniFactory::create(AsyncModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test 1: Async method with await
            let response = client
                .get(format!("http://127.0.0.1:{}/async/data", port))
                .send()
                .await
                .expect("Failed to get async data");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "async data");

            // Test 2: Async computation
            let response = client
                .get(format!("http://127.0.0.1:{}/async/compute", port))
                .send()
                .await
                .expect("Failed to compute");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Result: 84");

            // Test 3: Sync method still works
            let response = client
                .get(format!("http://127.0.0.1:{}/async/sync", port))
                .send()
                .await
                .expect("Failed to get sync response");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "sync response");

            // Test 4: Multiple awaits in one method
            let response = client
                .get(format!("http://127.0.0.1:{}/async/multi", port))
                .send()
                .await
                .expect("Failed to get multi response");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "async data - 20");
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_async_with_real_async_operation() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    // Service with actual async HTTP call
    #[injectable(
        pub struct HttpService;
    )]
    impl HttpService {
        pub async fn fetch_from_api(&self) -> String {
            // This is a real async operation using reqwest
            match reqwest::get("https://httpbin.org/get").await {
                Ok(_) => "API call successful".to_string(),
                Err(_) => "API call failed".to_string(),
            }
        }
    }

    #[controller(
        "/http",
        pub struct HttpController {
            #[inject]
            http_service: HttpService,
        }
    )]
    impl HttpController {
        #[get("/fetch")]
        async fn fetch(&self, _req: HttpRequest) -> ToniBody {
            let result = self.http_service.fetch_from_api().await;
            ToniBody::Text(result)
        }
    }

    #[module(
        controllers: [HttpController],
        providers: [HttpService],
    )]
    impl HttpModule {}

    let port = 29081;
    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let mut app = ToniFactory::create(HttpModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            let response = client
                .get(format!("http://127.0.0.1:{}/http/fetch", port))
                .send()
                .await
                .expect("Failed to fetch");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            // Either success or failure is fine - we just want to verify async works
            assert!(body == "API call successful" || body == "API call failed");
        })
        .await;
}
