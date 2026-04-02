
use crate::common::TestServer;
use serde::{Deserialize, Serialize};
use serial_test::serial;
use toni::{controller, extractors::Bytes, get, post, Body as ToniBody};

#[derive(Debug, Serialize, Deserialize)]
struct CreateUserDto {
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<usize>,
}

/// This controller demonstrates Toni's NestJS-style attribute-based parameter extraction.
///
/// Toni supports clean, attribute-based syntax similar to NestJS/Spring:
/// - `#[body]`: Extract JSON request body
/// - `#[param("name")]`: Extract path parameters
/// - `#[query("name")]`: Extract query string parameters
///
/// The macro automatically transforms these into proper type-safe extractors behind the scenes.
#[controller("/api", pub struct AttributeController {})]
impl AttributeController {
    /// Extract JSON body using #[body] attribute
    #[post("/users")]
    fn create_user(&self, #[body] dto: CreateUserDto) -> ToniBody {
        ToniBody::text(format!("Created user: {} <{}>", dto.name, dto.email))
    }

    /// Extract individual query parameters using #[query] attributes
    #[get("/search")]
    fn search(
        &self,
        #[query("q")] query: String,
        #[query("limit")] limit: Option<usize>,
    ) -> ToniBody {
        let limit = limit.unwrap_or(10);
        ToniBody::text(format!("Searching for '{}' with limit {}", query, limit))
    }

    /// Extract path parameter using #[param] attribute
    #[get("/users/{id}")]
    fn get_user(&self, #[param("id")] user_id: i32) -> ToniBody {
        ToniBody::text(format!("User ID: {}", user_id))
    }

    /// Extract ALL query params as struct using #[query] without argument
    #[get("/advanced-search")]
    fn advanced_search(&self, #[query] params: SearchParams) -> ToniBody {
        let limit = params.limit.unwrap_or(10);
        ToniBody::text(format!(
            "Advanced search: '{}' (limit: {})",
            params.q, limit
        ))
    }

    /// Test default values for query parameters
    #[get("/products")]
    fn list_products(
        &self,
        #[query("page", default = "1")] page: usize,
        #[query("pageSize", default = "20")] page_size: usize,
    ) -> ToniBody {
        ToniBody::text(format!("Products page {} (size: {})", page, page_size))
    }

    /// Mix multiple attribute extractors: #[param] + #[body]
    #[post("/users/{id}")]
    fn update_user(&self, #[param("id")] user_id: i32, #[body] dto: CreateUserDto) -> ToniBody {
        ToniBody::text(format!(
            "Updated user {}: {} <{}>",
            user_id, dto.name, dto.email
        ))
    }

    /// Extract binary data using Bytes extractor
    #[post("/upload")]
    fn upload_file(&self, data: Bytes) -> ToniBody {
        ToniBody::text(format!("Uploaded {} bytes", data.len()))
    }
}

#[toni::module(
    controllers: [AttributeController],
    providers: [],
)]
impl AttributeModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_body_attribute() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    let dto = CreateUserDto {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };

    let resp = server
        .client()
        .post(server.url("/api/users"))
        .json(&dto)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "Created user: Alice <alice@example.com>");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_query_attribute() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/api/search?q=rust&limit=20"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "Searching for 'rust' with limit 20");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_param_attribute() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/api/users/42"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "User ID: 42");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_query_struct_attribute() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/api/advanced-search?q=typescript&limit=50"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "Advanced search: 'typescript' (limit: 50)");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_default_values() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    // Test with no query params - should use defaults
    let resp = server
        .client()
        .get(server.url("/api/products"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "Products page 1 (size: 20)");

    // Test with partial params - should use default for missing one
    let resp2 = server
        .client()
        .get(server.url("/api/products?page=3"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp2.status(), 200);
    let body2 = resp2.text().await.unwrap();
    assert_eq!(body2, "Products page 3 (size: 20)");

    // Test with all params provided
    let resp3 = server
        .client()
        .get(server.url("/api/products?page=5&pageSize=50"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp3.status(), 200);
    let body3 = resp3.text().await.unwrap();
    assert_eq!(body3, "Products page 5 (size: 50)");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_mixed_attributes() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    let dto = CreateUserDto {
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
    };

    let resp = server
        .client()
        .post(server.url("/api/users/99"))
        .json(&dto)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "Updated user 99: Bob <bob@example.com>");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_binary_upload() {
    let server = TestServer::start(AttributeModule::module_definition()).await;

    // Create some binary data
    let binary_data = vec![0u8, 1, 2, 3, 4, 5, 255, 128, 64];

    let resp = server
        .client()
        .post(server.url("/api/upload"))
        .header("Content-Type", "application/octet-stream")
        .body(binary_data.clone())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, format!("Uploaded {} bytes", binary_data.len()));
}
