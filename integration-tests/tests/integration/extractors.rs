use crate::common::TestServer;
use serde::Deserialize;
use serial_test::serial;
use toni::{
    controller,
    extractors::{Json, Query, Validated},
    get, module, post, Body as ToniBody,
};
use validator::Validate;

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct CreateUserDto {
    name: String,
    email: String,
}

#[controller(
    "/api",
    pub struct ExtractorController;
)]
impl ExtractorController {
    #[get("/search")]
    fn search(&self, Query(params): Query<SearchParams>) -> ToniBody {
        let limit = params.limit.unwrap_or(10);
        ToniBody::text(format!("Searching for '{}' with limit {}", params.q, limit))
    }

    #[post("/users")]
    fn create_user(&self, Json(dto): Json<CreateUserDto>) -> ToniBody {
        ToniBody::text(format!("Created user: {} <{}>", dto.name, dto.email))
    }

    #[post("/echo")]
    fn echo_json(&self, body: Json<serde_json::Value>) -> ToniBody {
        ToniBody::json(body.into_inner())
    }
}

#[module(
    controllers: [ExtractorController],
    providers: [],
)]
impl ExtractorModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_query_extractor() {
    let server = TestServer::start(ExtractorModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/api/search?q=rust&limit=5"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "Searching for 'rust' with limit 5"
    );

    let resp = server
        .client()
        .get(server.url("/api/search?q=toni"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "Searching for 'toni' with limit 10"
    );

    // missing required param → 400
    let resp = server
        .client()
        .get(server.url("/api/search"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_json_extractor() {
    let server = TestServer::start(ExtractorModule::module_definition()).await;

    let resp = server
        .client()
        .post(server.url("/api/users"))
        .json(&serde_json::json!({"name": "John Doe", "email": "john@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "Created user: John Doe <john@example.com>"
    );

    // echo generic JSON value
    let resp = server
        .client()
        .post(server.url("/api/echo"))
        .json(&serde_json::json!({"foo": "bar", "num": 42}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["foo"], "bar");
    assert_eq!(body["num"], 42);

    // missing required field → 400
    let resp = server
        .client()
        .post(server.url("/api/users"))
        .json(&serde_json::json!({"name": "John Doe"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[derive(Debug, Deserialize, Validate)]
struct ValidatedUserDto {
    #[validate(length(min = 3, message = "Name must be at least 3 characters"))]
    name: String,
    #[validate(email(message = "Invalid email format"))]
    email: String,
}

#[controller(
    "/validated",
    pub struct ValidatedController;
)]
impl ValidatedController {
    #[post("/users")]
    fn create_user(&self, Validated(Json(dto)): Validated<Json<ValidatedUserDto>>) -> ToniBody {
        ToniBody::text(format!(
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

#[serial]
#[tokio_localset_test::localset_test]
async fn test_validated_extractor() {
    let server = TestServer::start(ValidatedModule::module_definition()).await;

    let resp = server
        .client()
        .post(server.url("/validated/users"))
        .json(&serde_json::json!({"name": "John Doe", "email": "john@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "Created validated user: John Doe <john@example.com>"
    );

    // name too short
    let resp = server
        .client()
        .post(server.url("/validated/users"))
        .json(&serde_json::json!({"name": "Jo", "email": "john@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // invalid email
    let resp = server
        .client()
        .post(server.url("/validated/users"))
        .json(&serde_json::json!({"name": "John Doe", "email": "not-an-email"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // both invalid
    let resp = server
        .client()
        .post(server.url("/validated/users"))
        .json(&serde_json::json!({"name": "Jo", "email": "bad"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
