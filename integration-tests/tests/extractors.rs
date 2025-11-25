//! Test extractors (Path, Query, Json, Validated)
//!
//! This test verifies that extractors work correctly with the controller macro

use serde::Deserialize;
use serial_test::serial;
use toni::{
    controller, controller_struct,
    extractors::{Json, Query, Validated},
    get, module, post, Body as ToniBody, HttpAdapter,
};
use toni_axum::AxumAdapter;
use validator::Validate;

// Query params DTO
#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<i32>,
}

// JSON body DTO
#[derive(Debug, Deserialize)]
struct CreateUserDto {
    name: String,
    email: String,
}

// Controller using extractors
#[controller_struct(
    pub struct ExtractorController;
)]
#[controller("/api")]
impl ExtractorController {
    // Test Query extractor with destructuring
    #[get("/search")]
    fn search(&self, Query(params): Query<SearchParams>) -> ToniBody {
        let limit = params.limit.unwrap_or(10);
        ToniBody::Text(format!("Searching for '{}' with limit {}", params.q, limit))
    }

    // Test Json extractor with destructuring
    #[post("/users")]
    fn create_user(&self, Json(dto): Json<CreateUserDto>) -> ToniBody {
        ToniBody::Text(format!("Created user: {} <{}>", dto.name, dto.email))
    }

    // Test Json extractor without destructuring
    #[post("/echo")]
    fn echo_json(&self, body: Json<serde_json::Value>) -> ToniBody {
        ToniBody::Json(body.into_inner())
    }
}

// Application module
#[module(
    controllers: [ExtractorController],
    providers: [],
)]
impl ExtractorModule {}

#[tokio::test]
#[serial]
async fn test_query_extractor() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29200;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(ExtractorModule::module_definition(), adapter)
            .await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test 1: Query extractor with all params
            let response = client
                .get(format!(
                    "http://127.0.0.1:{}/api/search?q=rust&limit=5",
                    port
                ))
                .send()
                .await
                .expect("Failed to search");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Searching for 'rust' with limit 5");

            // Test 2: Query extractor with optional param missing
            let response = client
                .get(format!("http://127.0.0.1:{}/api/search?q=toni", port))
                .send()
                .await
                .expect("Failed to search");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Searching for 'toni' with limit 10");

            // Test 3: Query extractor with missing required param should fail
            let response = client
                .get(format!("http://127.0.0.1:{}/api/search", port))
                .send()
                .await
                .expect("Failed to search");

            assert_eq!(response.status(), 400);
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_json_extractor() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29201;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(ExtractorModule::module_definition(), adapter)
            .await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test 1: Json extractor with valid body
            let response = client
                .post(format!("http://127.0.0.1:{}/api/users", port))
                .json(&serde_json::json!({
                    "name": "John Doe",
                    "email": "john@example.com"
                }))
                .send()
                .await
                .expect("Failed to create user");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Created user: John Doe <john@example.com>");

            // Test 2: Json extractor with generic Value
            let response = client
                .post(format!("http://127.0.0.1:{}/api/echo", port))
                .json(&serde_json::json!({
                    "foo": "bar",
                    "num": 42
                }))
                .send()
                .await
                .expect("Failed to echo");

            assert_eq!(response.status(), 200);
            let body: serde_json::Value = response.json().await.unwrap();
            assert_eq!(body["foo"], "bar");
            assert_eq!(body["num"], 42);

            // Test 3: Json extractor with invalid body should fail
            let response = client
                .post(format!("http://127.0.0.1:{}/api/users", port))
                .json(&serde_json::json!({
                    "name": "John Doe"
                    // missing email field
                }))
                .send()
                .await
                .expect("Failed to create user");

            assert_eq!(response.status(), 400);
        })
        .await;
}

// Validated DTO with validation rules
#[derive(Debug, Deserialize, Validate)]
struct ValidatedUserDto {
    #[validate(length(min = 3, message = "Name must be at least 3 characters"))]
    name: String,
    #[validate(email(message = "Invalid email format"))]
    email: String,
}

// Controller using Validated extractor
#[controller_struct(
    pub struct ValidatedController;
)]
#[controller("/validated")]
impl ValidatedController {
    #[post("/users")]
    fn create_user(&self, Validated(Json(dto)): Validated<Json<ValidatedUserDto>>) -> ToniBody {
        ToniBody::Text(format!(
            "Created validated user: {} <{}>",
            dto.name, dto.email
        ))
    }
}

#[module(
    controllers: [ValidatedController],
    providers: [],
)]
impl ValidatedModule {}

#[tokio::test]
#[serial]
async fn test_validated_extractor() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29202;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(ValidatedModule::module_definition(), adapter)
            .await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test 1: Valid data passes validation
            let response = client
                .post(format!("http://127.0.0.1:{}/validated/users", port))
                .json(&serde_json::json!({
                    "name": "John Doe",
                    "email": "john@example.com"
                }))
                .send()
                .await
                .expect("Failed to create user");

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Created validated user: John Doe <john@example.com>");

            // Test 2: Name too short fails validation
            let response = client
                .post(format!("http://127.0.0.1:{}/validated/users", port))
                .json(&serde_json::json!({
                    "name": "Jo",  // Too short, min 3
                    "email": "john@example.com"
                }))
                .send()
                .await
                .expect("Failed to create user");

            assert_eq!(response.status(), 400);

            // Test 3: Invalid email fails validation
            let response = client
                .post(format!("http://127.0.0.1:{}/validated/users", port))
                .json(&serde_json::json!({
                    "name": "John Doe",
                    "email": "not-an-email"  // Invalid email format
                }))
                .send()
                .await
                .expect("Failed to create user");

            assert_eq!(response.status(), 400);

            // Test 4: Both invalid fails validation
            let response = client
                .post(format!("http://127.0.0.1:{}/validated/users", port))
                .json(&serde_json::json!({
                    "name": "Jo",
                    "email": "bad"
                }))
                .send()
                .await
                .expect("Failed to create user");

            assert_eq!(response.status(), 400);
        })
        .await;
}
