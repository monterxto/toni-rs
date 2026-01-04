//! Example demonstrating error handling and response builder features
//!
//! This example shows:
//! 1. HttpResponse builder pattern usage
//! 2. HttpError enum for automatic error handling
//! 3. Result<T, HttpError> in controller handlers
//! 4. Custom error handlers
//!
//! Run with:
//! ```bash
//! cargo run --example error_handling_and_builder
//! ```

use serde_json::json;
use toni::{
    async_trait, controller, errors::HttpError, get, injectable, module, post,
    toni_factory::ToniFactory, traits_helpers::ErrorHandler, Body as ToniBody, HttpRequest,
    HttpResponse,
};
use toni_axum::AxumAdapter;

// ===== Example 1: Using HttpResponse Builder Pattern =====

#[controller("/api", pub struct BuilderController {})]
impl BuilderController {
    /// Simple OK response with JSON
    #[get("/hello")]
    fn hello(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::ok()
            .json(json!({"message": "Hello, World!"}))
            .build()
    }

    /// Response with custom headers
    #[get("/with-headers")]
    fn with_headers(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::ok()
            .header("X-Custom-Header", "CustomValue")
            .header("X-Request-ID", "12345")
            .json(json!({"status": "success"}))
            .build()
    }

    /// Created response (201)
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

    /// Text response
    #[get("/text")]
    fn text(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::ok().text("Plain text response").build()
    }

    /// Binary response
    #[get("/binary")]
    fn binary(&self, _req: HttpRequest) -> HttpResponse {
        let data = vec![0u8, 1, 2, 3, 4, 5];
        HttpResponse::ok()
            .header("Content-Type", "application/octet-stream")
            .binary(data)
            .build()
    }

    /// Custom status code
    #[get("/custom-status")]
    fn custom_status(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse::builder()
            .status(202) // Accepted
            .json(json!({"message": "Request accepted for processing"}))
            .build()
    }
}

// ===== Example 2: Using HttpError with Result<T, E> =====

#[injectable(pub struct UserService {})]
impl UserService {
    /// Simulate finding a user by ID
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

    /// Simulate authentication
    fn authenticate(&self, token: &str) -> Result<(), HttpError> {
        if token == "valid-token" {
            Ok(())
        } else if token.is_empty() {
            Err(HttpError::unauthorized("Authentication token required"))
        } else {
            Err(HttpError::forbidden("Invalid authentication token"))
        }
    }

    /// Simulate creating a user
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

#[controller("/users", pub struct UserController {
    #[inject]
    service: UserService,
})]
impl UserController {
    /// Get user by ID - Returns Result<ToniBody, HttpError>
    /// The ? operator automatically propagates errors
    #[get("/{id}")]
    fn get_user(&self, req: HttpRequest) -> Result<ToniBody, HttpError> {
        let id = req
            .path_params
            .get("id")
            .ok_or_else(|| HttpError::bad_request("Missing user ID"))?;

        let user = self.service.find_user(id)?; // ? automatically converts error
        Ok(ToniBody::Json(user))
    }

    /// Protected endpoint requiring authentication
    #[get("/me")]
    fn get_current_user(&self, req: HttpRequest) -> Result<ToniBody, HttpError> {
        let token = req
            .header("Authorization")
            .ok_or_else(|| HttpError::unauthorized("Authorization header required"))?;

        self.service.authenticate(token)?;

        Ok(ToniBody::Json(json!({
            "id": "current-user",
            "name": "Authenticated User"
        })))
    }

    /// Create user with validation
    #[post("/")]
    fn create_user(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        // Extract email from request body
        let email = if let ToniBody::Json(body) = &req.body {
            body.get("email")
                .and_then(|v| v.as_str())
                .ok_or_else(|| HttpError::bad_request("Email is required"))?
        } else {
            return Err(HttpError::bad_request("Invalid request body"));
        };

        let user = self.service.create_user(email)?;

        // Use builder pattern for 201 Created response
        Ok(HttpResponse::created()
            .header("Location", format!("/users/{}", user["id"]))
            .json(user)
            .build())
    }

    /// Example of custom error
    #[get("/special")]
    fn special(&self, _req: HttpRequest) -> Result<ToniBody, HttpError> {
        Err(HttpError::custom(418, "I'm a teapot"))
    }
}

// ===== Example 3: Custom Error Handler =====

pub struct CustomErrorHandler;

#[async_trait]
impl ErrorHandler for CustomErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        request: &HttpRequest,
    ) -> HttpResponse {
        // Check if it's an HttpError
        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            // Add custom fields to the error response
            return HttpResponse::builder()
                .status(http_error.status_code())
                .json(json!({
                    "statusCode": http_error.status_code(),
                    "message": http_error.message(),
                    "error": http_error.error_type(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "path": request.uri.clone(),
                    "method": request.method.clone(),
                }))
                .build();
        }

        // Fallback for unknown errors
        HttpResponse::internal_server_error()
            .json(json!({
                "statusCode": 500,
                "message": "An unexpected error occurred",
                "error": "Internal Server Error",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
            .build()
    }
}

// ===== Module Setup =====

#[module(
    controllers: [BuilderController, UserController],
    providers: [UserService],
)]
pub struct AppModule;

#[tokio::main]
async fn main() {
    println!("Starting server with error handling and builder examples...");
    println!("\nTry these endpoints:");
    println!("  GET  http://localhost:3000/api/hello");
    println!("  GET  http://localhost:3000/api/with-headers");
    println!("  POST http://localhost:3000/api/create");
    println!("  GET  http://localhost:3000/api/text");
    println!("  GET  http://localhost:3000/api/custom-status");
    println!("\n  GET  http://localhost:3000/users/1              (Success)");
    println!("  GET  http://localhost:3000/users/999            (404 Not Found)");
    println!("  GET  http://localhost:3000/users/invalid        (400 Bad Request)");
    println!(
        "  GET  http://localhost:3000/users/me             (401 Unauthorized - missing header)"
    );
    println!("  GET  http://localhost:3000/users/me -H 'Authorization: valid-token'");
    println!("  POST http://localhost:3000/users -d '{{\"email\":\"test@example.com\"}}'");
    println!("  POST http://localhost:3000/users -d '{{\"email\":\"existing@example.com\"}}' (409 Conflict)");
    println!("  GET  http://localhost:3000/users/special        (418 I'm a teapot)");

    let app = ToniFactory::create(AppModule, AxumAdapter::new()).await;

    app.listen(3000, "127.0.0.1").await;
}
