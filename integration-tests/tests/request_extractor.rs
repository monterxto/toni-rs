//! Test Request extractor - optional request parameter
//!
//! This test verifies that Request can be used as an optional extractor parameter,
//! allowing controller methods to only accept Request when they need it.

use serde::Deserialize;
use serial_test::serial;
use toni::{
    controller, extractors::Json, get, module, post, Body as ToniBody, Request,
};
use toni_axum::AxumAdapter;

#[derive(Debug, Deserialize)]
struct CreateDto {
    name: String,
}

// Controller demonstrating optional Request parameter
#[controller("/api", pub struct RequestExtractorController;)]
impl RequestExtractorController {
    // Method WITHOUT Request parameter - clean!
    #[get("/hello")]
    fn hello(&self) -> ToniBody {
        ToniBody::Text("Hello, World!".to_string())
    }

    // Method WITH Request parameter - only when needed
    #[get("/info")]
    fn get_info(&self, req: Request) -> ToniBody {
        let method = req.method();
        let uri = req.uri();
        ToniBody::Text(format!("Method: {}, URI: {}", method, uri))
    }

    // Method with Request AND other extractors
    #[post("/create")]
    fn create(&self, Json(dto): Json<CreateDto>, req: Request) -> ToniBody {
        let content_type = req.header("content-type").unwrap_or("unknown");
        ToniBody::Text(format!(
            "Created {} with content-type: {}",
            dto.name, content_type
        ))
    }

    // Method with custom header checking
    #[get("/protected")]
    fn protected(&self, req: Request) -> ToniBody {
        match req.header("authorization") {
            Some(auth) => ToniBody::Text(format!("Authorized: {}", auth)),
            None => ToniBody::Text("Unauthorized".to_string()),
        }
    }

    // Method accessing query params via Request
    #[get("/search")]
    fn search(&self, req: Request) -> ToniBody {
        let query_params = req.query_params();
        let q = query_params.get("q").map(|s| s.as_str()).unwrap_or("");
        ToniBody::Text(format!("Searching for: {}", q))
    }
}

#[module(controllers: [RequestExtractorController], providers: [],)]
impl RequestExtractorModule {}

#[tokio::test]
#[serial]
async fn test_method_without_request_parameter() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29300;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(RequestExtractorModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test method without Request parameter
            let response = client
                .get(format!("http://127.0.0.1:{}/api/hello", port))
                .send()
                .await
                .expect("Failed to call /hello");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Hello, World!");
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_method_with_request_parameter() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29301;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(RequestExtractorModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test method with Request parameter
            let response = client
                .get(format!("http://127.0.0.1:{}/api/info", port))
                .send()
                .await
                .expect("Failed to call /info");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("Method: GET"));
            assert!(body.contains("URI: /api/info"));
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_method_with_request_and_json_extractor() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29302;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(RequestExtractorModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test method with both Json and Request extractors
            let response = client
                .post(format!("http://127.0.0.1:{}/api/create", port))
                .json(&serde_json::json!({
                    "name": "Test User"
                }))
                .send()
                .await
                .expect("Failed to call /create");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("Created Test User"));
            assert!(body.contains("content-type"));
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_request_header_access() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29303;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(RequestExtractorModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test with authorization header
            let response = client
                .get(format!("http://127.0.0.1:{}/api/protected", port))
                .header("authorization", "Bearer token123")
                .send()
                .await
                .expect("Failed to call /protected");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("Authorized: Bearer token123"));

            // Test without authorization header
            let response = client
                .get(format!("http://127.0.0.1:{}/api/protected", port))
                .send()
                .await
                .expect("Failed to call /protected");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Unauthorized");
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_request_query_params_access() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29304;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(RequestExtractorModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test with query params
            let response = client
                .get(format!("http://127.0.0.1:{}/api/search?q=rust", port))
                .send()
                .await
                .expect("Failed to call /search");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Searching for: rust");
        })
        .await;
}
