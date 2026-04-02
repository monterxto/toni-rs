//! Test for Extensions and from_request pattern
//!
//! This demonstrates:
//! 1. Middleware adding typed data to request extensions
//! 2. Request-scoped providers using from_request to access that data
//! 3. Controllers using request context without manual extraction

use toni::{
    controller, get, http_helpers::RequestPart, injectable, module, toni_factory::ToniFactory,
    Body as ToniBody,
};

// ===== 1. Define types to store in extensions =====

#[derive(Clone, Debug)]
pub struct UserId(String);

#[derive(Clone, Debug)]
pub struct RequestId(String);

// ===== 2. Request-scoped provider using from_request =====

#[injectable(scope = "request", init = "from_request")]
pub struct RequestContext {
    user_id: String,
    request_id: String,
    is_authenticated: bool,
}

impl RequestContext {
    /// Special method called by framework with the request parts.
    pub fn from_request(req: &RequestPart) -> Self {
        let user_id = req
            .extensions
            .get::<UserId>()
            .map(|u| u.0.clone())
            .unwrap_or_else(|| "anonymous".to_string());

        let request_id = req
            .extensions
            .get::<RequestId>()
            .map(|r| r.0.clone())
            .unwrap_or_else(|| "no-request-id".to_string());

        let is_authenticated = user_id != "anonymous";

        Self {
            user_id,
            request_id,
            is_authenticated,
        }
    }

    pub fn require_auth(&self) -> Result<&str, &'static str> {
        if self.is_authenticated {
            Ok(&self.user_id)
        } else {
            Err("Unauthenticated")
        }
    }

    pub fn get_user_id(&self) -> &str {
        &self.user_id
    }

    pub fn get_request_id(&self) -> &str {
        &self.request_id
    }
}

// ===== 3. Singleton service (business logic) =====

#[injectable]
pub struct UserService {}

impl UserService {
    pub fn get_user_data(&self, user_id: &str) -> String {
        // Pure business logic - no HTTP coupling!
        format!("Data for user: {}", user_id)
    }
}

// ===== 4. Controller using request context =====

#[controller("/users", pub struct UserController {
    #[inject]
    context: RequestContext,  // Request-scoped context
    #[inject]
    user_service: UserService, // Singleton service
})]
impl UserController {
    #[get("/me")]
    fn get_current_user(&self) -> ToniBody {
        // No manual extraction! Context is already populated
        let user_id = self.context.get_user_id();
        let request_id = self.context.get_request_id();

        let data = self.user_service.get_user_data(user_id);

        ToniBody::text(format!(
            "Request ID: {}\nUser: {}\nData: {}",
            request_id, user_id, data
        ))
    }

    #[get("/protected")]
    fn protected_route(&self) -> ToniBody {
        // Easy auth check
        match self.context.require_auth() {
            Ok(user_id) => ToniBody::text(format!("Protected data for user: {}", user_id)),
            Err(msg) => ToniBody::text(msg.to_string()),
        }
    }
}

// ===== 5. Module definition =====

#[module(
    providers: [RequestContext, UserService],
    controllers: [UserController],
)]
impl TestModule {}

// ===== 6. Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_construction() {
        let (mut req, ()) = http::Request::builder()
            .method("GET")
            .uri("/test")
            .body(())
            .unwrap()
            .into_parts();

        req.extensions.insert(UserId("alice".to_string()));
        req.extensions.insert(RequestId("req-123".to_string()));

        // Call from_request
        let context = RequestContext::from_request(&req);

        assert_eq!(context.get_user_id(), "alice");
        assert_eq!(context.get_request_id(), "req-123");
        assert!(context.is_authenticated);
        assert!(context.require_auth().is_ok());
    }

    #[test]
    fn test_request_context_anonymous() {
        let (req, ()) = http::Request::builder()
            .method("GET")
            .uri("/test")
            .body(())
            .unwrap()
            .into_parts();

        let context = RequestContext::from_request(&req);

        assert_eq!(context.get_user_id(), "anonymous");
        assert!(!context.is_authenticated);
        assert!(context.require_auth().is_err());
    }

    #[test]
    fn test_user_service() {
        // Test singleton service can be tested independently
        let service = UserService {};
        let data = service.get_user_data("bob");
        assert_eq!(data, "Data for user: bob");
    }

    #[tokio::test]
    async fn test_di_resolves() {
        // Verify the module wires correctly: UserService (singleton) must resolve,
        // and its business logic must be callable without an HTTP server.
        let app = ToniFactory::create(TestModule::module_definition()).await;

        let service = app
            .get::<UserService>()
            .await
            .expect("UserService should resolve as singleton");
        assert_eq!(service.get_user_data("alice"), "Data for user: alice");
    }
}
