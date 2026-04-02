
use crate::common::TestServer;
use serde::Deserialize;
use serial_test::serial;
use toni::{controller, extractors::Json, get, module, post, Body as ToniBody, Request};

#[derive(Debug, Deserialize)]
struct CreateDto {
    name: String,
}

#[controller("/api", pub struct RequestExtractorController;)]
impl RequestExtractorController {
    #[get("/hello")]
    fn hello(&self) -> ToniBody {
        ToniBody::text("Hello, World!".to_string())
    }

    #[get("/info")]
    fn get_info(&self, req: Request) -> ToniBody {
        let method = req.method();
        let uri = req.uri();
        ToniBody::text(format!("Method: {}, URI: {}", method, uri))
    }

    #[post("/create")]
    fn create(&self, Json(dto): Json<CreateDto>, req: Request) -> ToniBody {
        let content_type = req.header("content-type").unwrap_or("unknown");
        ToniBody::text(format!(
            "Created {} with content-type: {}",
            dto.name, content_type
        ))
    }

    #[get("/protected")]
    fn protected(&self, req: Request) -> ToniBody {
        match req.header("authorization") {
            Some(auth) => ToniBody::text(format!("Authorized: {}", auth)),
            None => ToniBody::text("Unauthorized".to_string()),
        }
    }

    #[get("/search")]
    fn search(&self, req: Request) -> ToniBody {
        let q = req
            .query_params()
            .get("q")
            .map(|s| s.as_str())
            .unwrap_or("");
        ToniBody::text(format!("Searching for: {}", q))
    }
}

#[module(controllers: [RequestExtractorController], providers: [])]
impl RequestExtractorModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_extractor_variants() {
    let server = TestServer::start(RequestExtractorModule::module_definition()).await;

    // method with no Request parameter
    let resp = server
        .client()
        .get(server.url("/api/hello"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Hello, World!");

    // request with URI/method access
    let resp = server
        .client()
        .get(server.url("/api/info"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("Method: GET"));
    assert!(body.contains("URI: /api/info"));

    // request + json extractor together
    let resp = server
        .client()
        .post(server.url("/api/create"))
        .json(&serde_json::json!({"name": "Test User"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("Created Test User"));
    assert!(body.contains("content-type"));

    // header access — present
    let resp = server
        .client()
        .get(server.url("/api/protected"))
        .header("authorization", "Bearer token123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Authorized: Bearer token123");

    // header access — absent
    let resp = server
        .client()
        .get(server.url("/api/protected"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Unauthorized");

    // query param access via Request
    let resp = server
        .client()
        .get(server.url("/api/search?q=rust"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Searching for: rust");
}
