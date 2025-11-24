//! Example middleware implementations
//!
//! These middleware implementations demonstrate how to implement the Middleware trait.
//! They are provided as educational examples and starting points for your own implementations.
//!
//! ## Usage
//!
//! These are not production-ready implementations. Use them as reference for:
//! - Understanding the Middleware trait pattern
//! - Building your own custom middleware
//! - Contributing to third-party middleware crates

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use toni::{
    async_trait,
    http_helpers::{Body, HttpRequest, HttpResponse, IntoResponse},
    middleware::{Middleware, MiddlewareResult, Next},
};

// LOGGER MIDDLEWARE

/// Logging middleware - logs request/response info
pub struct LoggerMiddleware {
    pub log_body: bool,
}

impl LoggerMiddleware {
    pub fn new() -> Self {
        Self { log_body: false }
    }

    pub fn with_body_logging(mut self) -> Self {
        self.log_body = true;
        self
    }
}

impl Default for LoggerMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for LoggerMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        let start = Instant::now();
        let method = req.method.clone();
        let uri = req.uri.clone();

        println!("➡️  {} {}", method, uri);

        if self.log_body {
            match &req.body {
                Body::Text(text) => println!("   Body: {}", text),
                Body::Json(json) => println!("   Body: {}", json),
            }
        }

        let result = next.run(req).await;

        let duration = start.elapsed();

        match &result {
            Ok(_) => println!("✅ {} {} - {:?}", method, uri, duration),
            Err(e) => println!("❌ {} {} - {:?} - Error: {}", method, uri, duration, e),
        }

        result
    }
}

// CORS MIDDLEWARE

/// CORS middleware
pub struct CorsMiddleware {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: Option<u32>,
}

impl CorsMiddleware {
    pub fn new() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
            allow_credentials: false,
            max_age: Some(3600),
        }
    }

    pub fn allow_origin(mut self, origin: String) -> Self {
        self.allowed_origins = vec![origin];
        self
    }

    pub fn allow_origins(mut self, origins: Vec<String>) -> Self {
        self.allowed_origins = origins;
        self
    }

    pub fn allow_methods(mut self, methods: Vec<String>) -> Self {
        self.allowed_methods = methods;
        self
    }

    pub fn allow_credentials(mut self, allow: bool) -> Self {
        self.allow_credentials = allow;
        self
    }
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for CorsMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        // Handle preflight OPTIONS request
        if req.method == "OPTIONS" {
            let mut response = HttpResponse::new();
            response.status = 204;

            // Add CORS headers
            let origin = self
                .allowed_origins
                .first()
                .unwrap_or(&"*".to_string())
                .clone();
            response
                .headers
                .push(("Access-Control-Allow-Origin".to_string(), origin));
            response.headers.push((
                "Access-Control-Allow-Methods".to_string(),
                self.allowed_methods.join(", "),
            ));
            response.headers.push((
                "Access-Control-Allow-Headers".to_string(),
                self.allowed_headers.join(", "),
            ));

            if self.allow_credentials {
                response.headers.push((
                    "Access-Control-Allow-Credentials".to_string(),
                    "true".to_string(),
                ));
            }

            if let Some(max_age) = self.max_age {
                response
                    .headers
                    .push(("Access-Control-Max-Age".to_string(), max_age.to_string()));
            }

            return Ok(response);
        }

        // Process normal request and add CORS headers to response
        let result = next.run(req).await?;

        // Add CORS headers to response
        let mut response = result.to_response();

        let origin = self
            .allowed_origins
            .first()
            .unwrap_or(&"*".to_string())
            .clone();
        response
            .headers
            .push(("Access-Control-Allow-Origin".to_string(), origin));

        if self.allow_credentials {
            response.headers.push((
                "Access-Control-Allow-Credentials".to_string(),
                "true".to_string(),
            ));
        }

        Ok(response)
    }
}

// AUTH MIDDLEWARE

/// Authentication middleware - checks for valid token
pub struct AuthMiddleware {
    pub header_name: String,
    pub prefix: String,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            header_name: "Authorization".to_string(),
            prefix: "Bearer ".to_string(),
        }
    }

    pub fn with_header(mut self, header: String) -> Self {
        self.header_name = header;
        self
    }
}

impl Default for AuthMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for AuthMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        // Check for auth header
        let auth_header = req
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&self.header_name))
            .map(|(_, value)| value);

        if let Some(header_value) = auth_header {
            if header_value.starts_with(&self.prefix) {
                // Token exists, continue
                return next.run(req).await;
            }
        }

        // No valid auth token
        let mut response = HttpResponse::new();
        response.status = 401;
        response.body = Some(Body::Json(serde_json::json!({
            "error": "Unauthorized",
            "message": "Missing or invalid authentication token"
        })));

        Ok(response)
    }
}

// TIMEOUT MIDDLEWARE

/// Request timeout middleware
pub struct TimeoutMiddleware {
    pub timeout_ms: u64,
}

impl TimeoutMiddleware {
    pub fn new(timeout_ms: u64) -> Self {
        Self { timeout_ms }
    }
}

#[async_trait]
impl Middleware for TimeoutMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        let timeout_duration = std::time::Duration::from_millis(self.timeout_ms);

        match tokio::time::timeout(timeout_duration, next.run(req)).await {
            Ok(result) => result,
            Err(_) => {
                let mut response = HttpResponse::new();
                response.status = 408;
                response.body = Some(Body::Json(serde_json::json!({
                    "error": "Request Timeout",
                    "message": format!("Request exceeded timeout of {}ms", self.timeout_ms)
                })));
                Ok(response)
            }
        }
    }
}

// COMPRESSION MIDDLEWARE

/// Compression middleware (placeholder - not implemented)
pub struct CompressionMiddleware {
    pub level: u32,
}

impl CompressionMiddleware {
    pub fn new() -> Self {
        Self { level: 6 }
    }

    pub fn with_level(mut self, level: u32) -> Self {
        self.level = level;
        self
    }
}

impl Default for CompressionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for CompressionMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        // TODO: Implement actual compression
        // For now, just pass through
        next.run(req).await
    }
}

// RATE LIMITING MIDDLEWARE

/// Rate limiting middleware (simple in-memory implementation)
pub struct RateLimitMiddleware {
    max_requests: usize,
    window_ms: u64,
    // IP -> (request_count, window_start)
    store: Mutex<HashMap<String, (usize, Instant)>>,
}

impl RateLimitMiddleware {
    pub fn new(max_requests: usize, window_ms: u64) -> Self {
        Self {
            max_requests,
            window_ms,
            store: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Middleware for RateLimitMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        // Extract IP
        // TODO: Improve IP extraction, check X-Forwarded-For, etc.
        let ip = req
            .headers
            .iter()
            .find(|(k, _)| k == "X-Forwarded-For" || k == "X-Real-IP")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "default_ip".to_string());

        // Check rate limit in a separate scope to drop the lock before await
        let should_allow = {
            let mut store = self.store.lock().unwrap();
            let now = Instant::now();

            let result = if let Some((count, window_start)) = store.get_mut(&ip) {
                let elapsed = now.duration_since(*window_start).as_millis() as u64;

                if elapsed > self.window_ms {
                    // New window
                    *count = 1;
                    *window_start = now;
                    true
                } else if *count < self.max_requests {
                    *count += 1;
                    true
                } else {
                    false
                }
            } else {
                store.insert(ip.clone(), (1, now));
                true
            };

            result
        };

        if should_allow {
            next.run(req).await
        } else {
            let mut response = HttpResponse::new();
            response.status = 429;
            response.body = Some(Body::Json(serde_json::json!({
                "error": "Too Many Requests",
                "message": "Rate limit exceeded"
            })));
            Ok(response)
        }
    }
}

fn main() {
    // This file is for reference only
    println!(
        "See the middleware implementations above for examples of how to implement the Middleware trait."
    );
}
