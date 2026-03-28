//! Comprehensive error handling example
//!
//! Demonstrates:
//! 1. Global error handlers (via factory.use_global_error_handler)
//! 2. Controller-level error handlers (via #[use_error_handlers] on impl)
//! 3. Method-level error handlers (via #[use_error_handlers] on methods)
//! 4. Chain-of-responsibility pattern (handlers return Option)
//! 5. Specialized error handlers for different error types
//! 6. HttpError enum for controller errors
//! 7. HttpResponse builder pattern
//! 8. Guards causing errors
//! 9. DI-based error handlers (registered in providers and resolved from container)
//!
//! Run with:
//! ```bash
//! cargo run --example error_handling
//! ```

use serde_json::json;
use std::sync::Arc;
use toni::{
    async_trait, controller,
    enhancer::error_handler,
    errors::HttpError,
    get, injectable,
    injector::Context,
    module, post,
    toni_factory::ToniFactory,
    traits_helpers::{ErrorHandler, Guard},
    Body as ToniBody, HttpRequest, HttpResponse,
};
use toni_axum::AxumAdapter;
use toni_macros::{use_error_handlers, use_guards};

// Global error handler - catches all unhandled errors
pub struct GlobalErrorHandler;

#[async_trait]
impl ErrorHandler for GlobalErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        request: &toni::RequestPart,
    ) -> Option<HttpResponse> {
        eprintln!(
            "[GlobalErrorHandler] {} {}: {}",
            request.method,
            request.uri,
            error
        );

        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            return Some(http_error.to_response());
        }

        Some(
            HttpResponse::builder()
                .status(500)
                .json(json!({
                    "statusCode": 500,
                    "message": "An unexpected error occurred",
                    "error": "Internal Server Error",
                    "handler": "GlobalErrorHandler",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "path": request.uri.to_string(),
                }))
                .build(),
        )
    }
}

// Specialized handler for validation errors (400, 422)
pub struct ValidationErrorHandler;

#[async_trait]
impl ErrorHandler for ValidationErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        request: &toni::RequestPart,
    ) -> Option<HttpResponse> {
        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            let status = http_error.status_code();
            if matches!(status, 400 | 422) {
                eprintln!(
                    "[ValidationErrorHandler] Handling validation error on {}: {}",
                    request.uri,
                    error
                );
                return Some(
                    HttpResponse::builder()
                        .status(status)
                        .json(json!({
                            "statusCode": status,
                            "message": http_error.message(),
                            "error": "Validation Error",
                            "handler": "ValidationErrorHandler",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "path": request.uri.to_string(),
                        }))
                        .build(),
                );
            }
        }
        None
    }
}

// Specialized handler for database errors (409 conflict)
pub struct DatabaseErrorHandler;

#[async_trait]
impl ErrorHandler for DatabaseErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        request: &toni::RequestPart,
    ) -> Option<HttpResponse> {
        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            if http_error.status_code() == 409 {
                eprintln!(
                    "[DatabaseErrorHandler] Handling conflict error on {}: {}",
                    request.uri,
                    error
                );
                return Some(
                    HttpResponse::builder()
                        .status(409)
                        .json(json!({
                            "statusCode": 409,
                            "message": http_error.message(),
                            "error": "Database Conflict",
                            "handler": "DatabaseErrorHandler",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "path": request.uri.to_string(),
                        }))
                        .build(),
                );
            }
        }
        None
    }
}

// Controller-level error handler for user operations (direct instantiation)
pub struct UserControllerErrorHandler;

#[async_trait]
impl ErrorHandler for UserControllerErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        request: &toni::RequestPart,
    ) -> Option<HttpResponse> {
        eprintln!(
            "[UserControllerErrorHandler] {} {}: {}",
            request.method,
            request.uri,
            error
        );

        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            return Some(
                HttpResponse::builder()
                    .status(http_error.status_code())
                    .json(json!({
                        "statusCode": http_error.status_code(),
                        "message": http_error.message(),
                        "handler": "UserControllerErrorHandler",
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "path": request.uri.to_string(),
                    }))
                    .build(),
            );
        }

        None
    }
}

// DI-based error handler - registered in providers and resolved from container
#[injectable(pub struct NotFoundErrorHandler {})]
#[error_handler]
impl NotFoundErrorHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ErrorHandler for NotFoundErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        request: &toni::RequestPart,
    ) -> Option<HttpResponse> {
        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            if http_error.status_code() == 404 {
                eprintln!(
                    "[NotFoundErrorHandler - DI] Handling 404 on {}: {}",
                    request.uri,
                    error
                );
                return Some(
                    HttpResponse::builder()
                        .status(404)
                        .json(json!({
                            "statusCode": 404,
                            "message": http_error.message(),
                            "error": "Not Found",
                            "handler": "NotFoundErrorHandler (DI-based)",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "path": request.uri.to_string(),
                        }))
                        .build(),
                );
            }
        }
        None
    }
}

pub struct AuthGuard;

impl Guard for AuthGuard {
    fn can_activate(&self, context: &Context) -> bool {
        context.take_request().headers.contains_key("x-auth-token")
    }
}

#[injectable(pub struct UserService {})]
impl UserService {
    fn find_user(&self, id: &str) -> Result<serde_json::Value, HttpError> {
        if id == "1" {
            Ok(json!({
                "id": "1",
                "name": "John Doe",
                "email": "john@example.com"
            }))
        } else if id == "invalid" {
            Err(HttpError::bad_request("Invalid user ID format"))
        } else {
            Err(HttpError::not_found(format!("User {} not found", id)))
        }
    }

    fn authenticate(&self, token: &str) -> Result<(), HttpError> {
        if token == "valid-token" {
            Ok(())
        } else if token.is_empty() {
            Err(HttpError::unauthorized("Authentication token required"))
        } else {
            Err(HttpError::forbidden("Invalid authentication token"))
        }
    }

    fn create_user(&self, email: &str) -> Result<serde_json::Value, HttpError> {
        if email == "existing@example.com" {
            Err(HttpError::conflict("User with this email already exists"))
        } else if !email.contains('@') {
            Err(HttpError::unprocessable_entity("Invalid email format"))
        } else {
            Ok(json!({
                "id": "new-123",
                "email": email,
                "created": true
            }))
        }
    }
}

#[controller("/api", pub struct ApiController {})]
impl ApiController {
    #[get("/hello")]
    fn hello(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::ok()
            .json(json!({"message": "Hello, World!"}))
            .build()
    }

    #[get("/with-headers")]
    fn with_headers(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::ok()
            .header("X-Custom-Header", "CustomValue")
            .header("X-Request-ID", "12345")
            .json(json!({"status": "success"}))
            .build()
    }

    #[post("/create")]
    fn create(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::created()
            .header("Location", "/api/resource/123")
            .json(json!({
                "id": 123,
                "message": "Resource created"
            }))
            .build()
    }

    #[get("/protected")]
    #[use_guards(AuthGuard{})]
    fn protected(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::json(json!({"message": "Access granted"}))
    }

    #[get("/not-found")]
    fn not_found(&self, _req: HttpRequest) -> Result<ToniBody, HttpError> {
        Err(HttpError::not_found("Resource not found"))
    }

    #[get("/server-error")]
    fn server_error(&self, _req: HttpRequest) -> Result<ToniBody, HttpError> {
        Err(HttpError::internal_server_error("Something went wrong"))
    }
}

#[controller("/users", pub struct UserController {
    #[inject]
    service: UserService,
})]
#[use_error_handlers(UserControllerErrorHandler{})]
impl UserController {
    #[get("/{id}")]
    fn get_user(&self, req: HttpRequest) -> Result<ToniBody, HttpError> {
        let id = req
            .extensions()
            .get::<toni::http_helpers::PathParams>()
            .and_then(|p| p.0.get("id").map(|s| s.as_str()))
            .ok_or_else(|| HttpError::bad_request("Missing user ID"))?;

        let user = self.service.find_user(id)?;
        Ok(ToniBody::json(user))
    }

    #[get("/me")]
    fn get_current_user(&self, req: HttpRequest) -> Result<ToniBody, HttpError> {
        let token = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| HttpError::unauthorized("Authorization header required"))?;

        self.service.authenticate(token)?;

        Ok(ToniBody::json(json!({
            "id": "current-user",
            "name": "Authenticated User"
        })))
    }

    #[post("/")]
    #[use_error_handlers(ValidationErrorHandler{}, DatabaseErrorHandler{})]
    async fn create_user(&self, toni::extractors::Json(body): toni::extractors::Json<serde_json::Value>) -> Result<HttpResponse, HttpError> {
        let email = body
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HttpError::bad_request("Email is required"))?;

        let user = self.service.create_user(email)?;

        Ok(HttpResponse::created()
            .header("Location", format!("/users/{}", user["id"]))
            .json(user)
            .build())
    }

    #[get("/special")]
    fn special(&self, _req: HttpRequest) -> Result<ToniBody, HttpError> {
        Err(HttpError::custom(418, "I'm a teapot"))
    }
}

// Demonstrates DI-based error handler using type name only (no direct instantiation)
#[controller("/products", pub struct ProductController {})]
#[use_error_handlers(NotFoundErrorHandler)]
impl ProductController {
    #[get("/{id}")]
    fn get_product(&self, req: HttpRequest) -> Result<ToniBody, HttpError> {
        let id = req
            .extensions()
            .get::<toni::http_helpers::PathParams>()
            .and_then(|p| p.0.get("id").map(|s| s.as_str()))
            .ok_or_else(|| HttpError::bad_request("Missing product ID"))?;

        if id == "1" {
            Ok(ToniBody::json(json!({
                "id": "1",
                "name": "Widget",
                "price": 19.99
            })))
        } else {
            Err(HttpError::not_found(format!("Product {} not found", id)))
        }
    }

    #[get("/")]
    fn list_products(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::json(json!([
            {"id": "1", "name": "Widget", "price": 19.99}
        ]))
    }
}

#[module(
    controllers: [ApiController, UserController, ProductController],
    providers: [UserService, NotFoundErrorHandler],
)]
pub struct AppModule;

#[tokio::main]
async fn main() {
    println!("Server running on http://localhost:3000\n");
    println!("HttpResponse builder examples:");
    println!("  GET  http://localhost:3000/api/hello");
    println!("  GET  http://localhost:3000/api/with-headers");
    println!("  POST http://localhost:3000/api/create\n");

    println!("Guard errors:");
    println!("  GET  http://localhost:3000/api/protected       → 403 (no X-Auth-Token)\n");

    println!("HttpError examples:");
    println!("  GET  http://localhost:3000/users/1             → 200 (success)");
    println!("  GET  http://localhost:3000/users/999           → 404 (not found)");
    println!("  GET  http://localhost:3000/users/invalid       → 400 (bad request)");
    println!("  GET  http://localhost:3000/users/me            → 401 (unauthorized)");
    println!("  GET  http://localhost:3000/users/me -H 'Authorization: valid-token'");
    println!("  POST http://localhost:3000/users -d '{{\"email\":\"test@example.com\"}}'");
    println!(
        "  POST http://localhost:3000/users -d '{{\"email\":\"existing@example.com\"}}' → 409"
    );
    println!("  GET  http://localhost:3000/users/special       → 418 (I'm a teapot)");
    println!("  GET  http://localhost:3000/api/server-error    → 500\n");

    println!("Error Handler Levels (Chain-of-Responsibility):");
    println!("  Method-level:");
    println!("    - POST /users uses ValidationErrorHandler → DatabaseErrorHandler");
    println!("  Controller-level:");
    println!("    - UserController uses UserControllerErrorHandler");
    println!("  Global-level:");
    println!("    - GlobalErrorHandler (via factory.use_global_error_handler)\n");

    println!("Execution order for POST /users errors:");
    println!("  1. ValidationErrorHandler (checks for 400/422)");
    println!("  2. DatabaseErrorHandler (checks for 409)");
    println!("  3. UserControllerErrorHandler (catches other user errors)");
    println!("  4. GlobalErrorHandler (final fallback)\n");

    println!("Test the chain-of-responsibility:");
    println!("  POST /users -d '{{\"email\":\"\"}}' → ValidationErrorHandler (400)");
    println!(
        "  POST /users -d '{{\"email\":\"existing@example.com\"}}' → DatabaseErrorHandler (409)"
    );
    println!("  GET /users/404 → UserControllerErrorHandler (404)");
    println!("  GET /api/server-error → GlobalErrorHandler (500)\n");

    println!("DI-based error handlers:");
    println!("  ProductController uses NotFoundErrorHandler (resolved from DI container)");
    println!("  GET  http://localhost:3000/products/1      → 200 (success)");
    println!("  GET  http://localhost:3000/products/999    → 404 (NotFoundErrorHandler via DI)");
    println!("  GET  http://localhost:3000/products        → 200 (list all)\n");

    println!("Two approaches to error handlers:");
    println!("  1. Direct instantiation: #[use_error_handlers(Handler{{}})]");
    println!("     - Used by UserController, ApiController");
    println!("  2. DI resolution: #[use_error_handlers(Handler)]");
    println!("     - Requires #[error_handler] marker attribute");
    println!("     - Registered in module providers");
    println!("     - Used by ProductController\n");

    let mut factory = ToniFactory::new();
    factory.use_global_error_handler(Arc::new(GlobalErrorHandler));

    let mut app = factory.create_with(AppModule).await;

    app.use_http_adapter(AxumAdapter::new("127.0.0.1", 3000)).unwrap();

    app.start().await;
}
