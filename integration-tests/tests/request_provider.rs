/// Test that the built-in Request provider is automatically available
/// in all modules without needing to explicitly add it to providers list.
use toni::{
    controller, controller_struct, get, module, Body as ToniBody, HttpAdapter, HttpRequest, Request,
};

#[controller_struct(pub struct TestController {
    #[inject]
    request: Request, // Request is automatically available - no need to add to providers!
})]
#[controller("/test")]
impl TestController {
    #[get("/info")]
    fn get_info(&self, _req: HttpRequest) -> ToniBody {
        // Access request data through the injected Request provider
        let method = self.request.method();
        let uri = self.request.uri();
        ToniBody::Text(format!("Method: {}, URI: {}", method, uri))
    }

    #[get("/headers")]
    fn get_headers(&self, _req: HttpRequest) -> ToniBody {
        // Access headers through the Request provider
        let content_type = self.request.header("content-type").unwrap_or("not found");
        ToniBody::Text(format!("Content-Type: {}", content_type))
    }
}

// Note: Request is NOT in the providers list - it's auto-injected by the framework!
#[module(
    imports: [],
    controllers: [TestController],
    providers: [], // Empty! Request still works
    exports: []
)]
impl TestModule {}

#[tokio::test]
async fn test_request_provider_e2e() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;
    use toni_axum::AxumAdapter;

    let port = 18081;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();

        let app = ToniFactory::create(TestModule::module_definition(), adapter).await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test that Request provider gives us access to method and URI
            let response = client
                .get(format!("http://127.0.0.1:{}/test/info", port))
                .send()
                .await
                .expect("GET request failed");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("Method: GET"));
            assert!(body.contains("URI: /test/info"));

            // Test that Request provider gives us access to headers
            let response = client
                .get(format!("http://127.0.0.1:{}/test/headers", port))
                .header("Content-Type", "application/json")
                .send()
                .await
                .expect("GET request with headers failed");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("Content-Type: application/json"));
        })
        .await;
}
