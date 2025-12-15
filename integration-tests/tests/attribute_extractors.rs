mod common;

use common::TestServer;
use serde::{Deserialize, Serialize};
use serial_test::serial;
use toni::{controller, controller_struct, get, post, Body as ToniBody, HttpRequest};

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
#[controller_struct(pub struct AttributeController {})]
#[controller("/api")]
impl AttributeController {
    /// Extract JSON body using #[body] attribute
    #[post("/users")]
    fn create_user(&self, #[body] dto: CreateUserDto) -> ToniBody {
        ToniBody::Text(format!("Created user: {} <{}>", dto.name, dto.email))
    }

    /// Extract individual query parameters using #[query] attributes
    #[get("/search")]
    fn search(
        &self,
        #[query("q")] query: String,
        #[query("limit")] limit: Option<usize>,
    ) -> ToniBody {
        let limit = limit.unwrap_or(10);
        ToniBody::Text(format!("Searching for '{}' with limit {}", query, limit))
    }

    /// Extract path parameter using #[param] attribute
    #[get("/users/{id}")]
    fn get_user(&self, #[param("id")] user_id: i32) -> ToniBody {
        ToniBody::Text(format!("User ID: {}", user_id))
    }

    /// Mix multiple attribute extractors: #[param] + #[body] + HttpRequest
    #[post("/users/{id}")]
    fn update_user(
        &self,
        #[param("id")] user_id: i32,
        #[body] dto: CreateUserDto,
        _req: HttpRequest,
    ) -> ToniBody {
        ToniBody::Text(format!(
            "Updated user {}: {} <{}>",
            user_id, dto.name, dto.email
        ))
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
