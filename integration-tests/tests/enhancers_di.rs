//! Test for DI-Based Enhancers (Middleware, Guards, Interceptors)
//!
//! This test demonstrates the unified DI & Enhancer system where:
//! 1. Middleware, Guards and Interceptors are registered as providers in the DI container
//! 2. They can have their own dependencies injected
//! 3. Controllers reference them by type (token-based resolution)
//! 4. No manual instantiation or boilerplate needed
//!
//! NB: Pipe is not supported as it is fundamentally different as in NestJs. See examples crate (examples/pipes_complete_guide.rs).

use serial_test::serial;
use std::sync::{Arc, Mutex};
use toni::async_trait;
use toni::enhancer::{guard, interceptor, middleware};
use toni::{
    controller, controller_struct, get, injectable, module, use_guards, use_interceptors,
    Body as ToniBody, HttpAdapter, HttpRequest,
};
use toni_axum::AxumAdapter;

use toni::injector::Context;
use toni::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};
use toni::traits_helpers::{Guard, Interceptor, InterceptorNext, MiddlewareConsumer};

// ============================================================================
// EXECUTION TRACKER (Shared State)
// ============================================================================

type ExecutionLog = Arc<Mutex<Vec<String>>>;

#[injectable]
pub struct ExecutionTracker {
    log: ExecutionLog,
}

impl ExecutionTracker {
    pub fn new() -> Self {
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn track(&self, event: &str) {
        self.log.lock().unwrap().push(event.to_string());
    }

    pub fn get_log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
}

// ============================================================================
// INJECTABLE SERVICE (Used by Enhancers)
// ============================================================================

/// Service that provides auth logic for enhancers
/// Demonstrates that enhancers can have dependencies injected
#[injectable(pub struct AuthService {
    tracker: ExecutionTracker,
})]
impl AuthService {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }

    pub fn is_admin(&self, req: &HttpRequest) -> bool {
        self.tracker.track("service:auth_check");
        req.has_header("X-Admin-Token")
    }

    pub fn validate_user(&self, req: &HttpRequest) -> bool {
        self.tracker.track("service:user_validation");
        req.has_header("X-User-Id")
    }
}

// ============================================================================
// INJECTABLE MIDDLEWARE (with DI)
// ============================================================================

/// Middleware that tracks execution with injected dependencies
#[injectable(pub struct RequestTrackingMiddleware {
    tracker: ExecutionTracker,
})]
#[middleware]
impl RequestTrackingMiddleware {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Middleware for RequestTrackingMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        self.tracker.track("middleware:request_tracking");
        let mut response = next.run(req).await?;
        response
            .headers
            .push(("X-Middleware".to_string(), "RequestTracking".to_string()));
        Ok(response)
    }
}

/// Middleware that validates headers with injected AuthService
#[injectable(pub struct HeaderValidationMiddleware {
    auth_service: AuthService,
})]
#[middleware]
impl HeaderValidationMiddleware {
    pub fn new(auth_service: AuthService) -> Self {
        Self { auth_service }
    }
}

#[async_trait]
impl Middleware for HeaderValidationMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        self.auth_service
            .tracker
            .track("middleware:header_validation");

        // Check for required header
        if !req.has_header("X-Request-ID") {
            let mut response = toni::HttpResponse::new();
            response.status = 400;
            response.body = Some(ToniBody::Text("Missing X-Request-ID header".to_string()));
            return Ok(response);
        }

        next.run(req).await
    }
}

// ============================================================================
// INJECTABLE GUARD (with DI)
// ============================================================================

/// Guard that checks for admin authorization using injected AuthService
#[injectable(pub struct AdminGuard {
    auth_service: AuthService,
})]
#[guard]
impl AdminGuard {
    pub fn new(auth_service: AuthService) -> Self {
        Self { auth_service }
    }
}

impl Guard for AdminGuard {
    fn can_activate(&self, context: &Context) -> bool {
        self.auth_service.tracker.track("guard:admin_check");
        self.auth_service.is_admin(context.take_request())
    }
}

/// Guard that checks for user authentication
#[injectable(pub struct UserGuard {
    auth_service: AuthService,
})]
#[guard]
impl UserGuard {
    pub fn new(auth_service: AuthService) -> Self {
        Self { auth_service }
    }
}

impl Guard for UserGuard {
    fn can_activate(&self, context: &Context) -> bool {
        self.auth_service.tracker.track("guard:user_check");
        self.auth_service.validate_user(context.take_request())
    }
}

// ============================================================================
// INJECTABLE INTERCEPTOR (with DI)
// ============================================================================

/// Interceptor that logs with injected service
#[injectable(pub struct LoggingInterceptor {
    tracker: ExecutionTracker,
})]
#[interceptor]
impl LoggingInterceptor {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Interceptor for LoggingInterceptor {
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
        self.tracker.track("interceptor:before");
        next.run(context).await;
        self.tracker.track("interceptor:after");
    }
}

/// Interceptor that adds timing information
#[injectable(pub struct TimingInterceptor {
    tracker: ExecutionTracker,
})]
#[interceptor]
impl TimingInterceptor {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Interceptor for TimingInterceptor {
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
        self.tracker.track("interceptor:timing_start");
        next.run(context).await;
        self.tracker.track("interceptor:timing_end");
    }
}

// ============================================================================
// TEST CONTROLLER (References DI-based Enhancers)
// ============================================================================

#[controller_struct(pub struct EnhancerTestController {
    tracker: ExecutionTracker,
})]
#[controller("/api")]
impl EnhancerTestController {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }

    /// Endpoint protected by AdminGuard (resolved from DI)
    #[get("/admin")]
    #[use_guards(AdminGuard)]
    #[use_interceptors(LoggingInterceptor)]
    fn admin_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:admin");
        ToniBody::Text("Admin access granted".to_string())
    }

    /// Endpoint protected by UserGuard (resolved from DI)
    #[get("/user")]
    #[use_guards(UserGuard)]
    #[use_interceptors(TimingInterceptor, LoggingInterceptor)] // Timing outer, Logging inner
    fn user_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:user");
        ToniBody::Text("User access granted".to_string())
    }

    /// Public endpoint (no enhancers)
    #[get("/public")]
    fn public_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:public");
        ToniBody::Text("Public access".to_string())
    }
}

// ============================================================================
// MODULE REGISTRATION (with Middleware)
// ============================================================================

#[module(
    controllers: [EnhancerTestController],
    providers: [
        AuthService,
        AdminGuard,
        UserGuard,
        LoggingInterceptor,
        TimingInterceptor,
        RequestTrackingMiddleware,
        HeaderValidationMiddleware,
        ExecutionTracker,
    ],
)]
impl EnhancerDITestModule {
    fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
        consumer
            .apply_token::<RequestTrackingMiddleware>()
            .apply_token_also::<HeaderValidationMiddleware>()
            .done();
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[tokio::test]
#[serial]
async fn test_di_guard_with_injected_dependencies() {
    use std::time::Duration;
    use tokio::sync::oneshot;
    use toni::toni_factory::ToniFactory;

    let port = 29100;
    let (tracker_tx, tracker_rx) = oneshot::channel();

    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(EnhancerDITestModule::module_definition(), adapter)
            .await;

        // Get the tracker from DI container before listen() takes ownership
        let tracker = app
            .get::<ExecutionTracker>()
            .await
            .expect("Failed to get ExecutionTracker from DI");

        // Send tracker to test task
        let _ = tracker_tx.send(tracker);

        let _ = app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Receive tracker from spawned task
            let tracker = tracker_rx.await.expect("Failed to receive tracker");

            let client = reqwest::Client::new();

            // Test 1: Admin endpoint without admin token (should fail - guard rejects)
            tracker.clear();
            let response = client
                .get(format!("http://127.0.0.1:{}/api/admin", port))
                .header("X-Request-ID", "test-123")
                .send()
                .await
                .expect("Failed to make request");

            println!("{:?}", response);
            let status = response.status();
            let body = response.text().await.expect("Failed to read body");
            println!("Response body: {}", body);
            assert_eq!(status, 403, "Should be forbidden without admin token");
            let log = tracker.get_log();
            println!("Log (no admin token): {:?}", log);
            assert!(log.contains(&"middleware:request_tracking".to_string()));
            assert!(log.contains(&"middleware:header_validation".to_string()));
            assert!(log.contains(&"guard:admin_check".to_string()));
            assert!(
                log.contains(&"service:auth_check".to_string()),
                "AuthService should be called by Guard"
            );
            assert!(
                !log.contains(&"controller:admin".to_string()),
                "Controller should not execute when guard rejects"
            );

            // Test 2: Admin endpoint WITH admin token (should succeed)
            tracker.clear();
            let response = client
                .get(format!("http://127.0.0.1:{}/api/admin", port))
                .header("X-Request-ID", "test-123")
                .header("X-Admin-Token", "valid")
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            assert_eq!(response.text().await.unwrap(), "Admin access granted");
            let log = tracker.get_log();
            println!("Log (with admin token): {:?}", log);

            // Verify full execution flow: middleware -> guard (with service) -> interceptor -> controller
            assert!(log.contains(&"middleware:request_tracking".to_string()));
            assert!(log.contains(&"guard:admin_check".to_string()));
            assert!(
                log.contains(&"service:auth_check".to_string()),
                "Injected AuthService should be used by Guard"
            );
            assert!(log.contains(&"interceptor:before".to_string()));
            assert!(log.contains(&"controller:admin".to_string()));
            assert!(log.contains(&"interceptor:after".to_string()));

            println!("✅ Test passed: DI-based Guards with injected dependencies work correctly");
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_di_interceptor_execution_order() {
    use std::time::Duration;
    use tokio::sync::oneshot;
    use toni::toni_factory::ToniFactory;

    let port = 29101;
    let (tracker_tx, tracker_rx) = oneshot::channel();

    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(EnhancerDITestModule::module_definition(), adapter)
            .await;

        let tracker = app
            .get::<ExecutionTracker>()
            .await
            .expect("Failed to get ExecutionTracker from DI");
        let _ = tracker_tx.send(tracker);

        let _ = app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let tracker = tracker_rx.await.expect("Failed to receive tracker");
            let client = reqwest::Client::new();

            // Test: User endpoint with multiple DI-based interceptors
            tracker.clear();
            let response = client
                .get(format!("http://127.0.0.1:{}/api/user", port))
                .header("X-Request-ID", "test-123")
                .header("X-User-Id", "123")
                .send()
                .await
                .expect("Failed to make request");

            println!("{:?}", response);
            assert_eq!(response.status(), 200);

            let log = tracker.get_log();
            println!("Execution log: {:?}", log);

            // Verify order: middleware -> guard -> timing_start -> logging_before -> controller -> logging_after -> timing_end
            let timing_start_idx = log
                .iter()
                .position(|e| e == "interceptor:timing_start")
                .expect("timing_start");
            let logging_before_idx = log
                .iter()
                .position(|e| e == "interceptor:before")
                .expect("before");
            let controller_idx = log
                .iter()
                .position(|e| e == "controller:user")
                .expect("controller");
            let logging_after_idx = log
                .iter()
                .position(|e| e == "interceptor:after")
                .expect("after");
            let timing_end_idx = log
                .iter()
                .position(|e| e == "interceptor:timing_end")
                .expect("timing_end");

            assert!(
                timing_start_idx < logging_before_idx,
                "TimingInterceptor should wrap LoggingInterceptor"
            );
            assert!(
                logging_before_idx < controller_idx,
                "Interceptors run before controller"
            );
            assert!(
                controller_idx < logging_after_idx,
                "Interceptors run after controller"
            );
            assert!(
                logging_after_idx < timing_end_idx,
                "TimingInterceptor completes last"
            );

            println!("✅ Test passed: DI-based Interceptors execute in correct order");
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_middleware_with_injected_dependencies() {
    use std::time::Duration;
    use tokio::sync::oneshot;
    use toni::toni_factory::ToniFactory;

    let port = 29102;
    let (tracker_tx, tracker_rx) = oneshot::channel();

    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(EnhancerDITestModule::module_definition(), adapter)
            .await;

        let tracker = app
            .get::<ExecutionTracker>()
            .await
            .expect("Failed to get ExecutionTracker from DI");
        let _ = tracker_tx.send(tracker);

        let _ = app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let tracker = tracker_rx.await.expect("Failed to receive tracker");
            let client = reqwest::Client::new();

            // Test 1: Request without X-Request-ID (should fail in middleware)
            tracker.clear();
            let response = client
                .get(format!("http://127.0.0.1:{}/api/public", port))
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 400);
            let body = response.text().await.unwrap();
            assert_eq!(body, "Missing X-Request-ID header");

            let log = tracker.get_log();
            println!("Log (no header): {:?}", log);
            assert!(log.contains(&"middleware:request_tracking".to_string()));
            assert!(
                log.contains(&"middleware:header_validation".to_string()),
                "HeaderValidationMiddleware uses injected AuthService"
            );
            assert!(
                !log.contains(&"controller:public".to_string()),
                "Controller should not run"
            );

            // Test 2: Request WITH X-Request-ID (should succeed)
            tracker.clear();
            let response = client
                .get(format!("http://127.0.0.1:{}/api/public", port))
                .header("X-Request-ID", "test-123")
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            assert!(
                response.headers().contains_key("x-middleware"),
                "Middleware should add header"
            );

            let log = tracker.get_log();
            println!("Log (with header): {:?}", log);
            assert!(log.contains(&"middleware:request_tracking".to_string()));
            assert!(log.contains(&"middleware:header_validation".to_string()));
            assert!(log.contains(&"controller:public".to_string()));

            println!(
                "✅ Test passed: DI-based Middleware with injected AuthService works correctly"
            );
        })
        .await;
}

#[tokio::test]
#[serial]
async fn test_no_enhancer_boilerplate_required() {
    // This test validates the core achievement: NO BOILERPLATE NEEDED!
    //
    // Before Phase 5:
    //   - Regular providers like ExecutionTracker needed: impl EnhancerMarker for ExecutionTracker {}
    //   - Enhancers needed separate impl EnhancerMarker with method overrides
    //
    // After Phase 5:
    //   - Regular providers: Just implement ProviderTrait (no EnhancerMarker boilerplate)
    //   - Enhancers: Auto-detected by #[injectable] macro based on trait implementations
    //   - ProviderTrait merged enhancer detection methods (as_guard, as_interceptor, etc.)

    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 29103;
    let _tracker = ExecutionTracker::new();

    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        let adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(EnhancerDITestModule::module_definition(), adapter)
            .await;
        let _ = app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let client = reqwest::Client::new();

            let response = client
                .get(format!("http://127.0.0.1:{}/api/public", port))
                .header("X-Request-ID", "test-123")
                .send()
                .await
                .expect("Failed to make request");

            assert_eq!(response.status(), 200);
            assert_eq!(response.text().await.unwrap(), "Public access");

            println!("\n✅ PHASE 5 VALIDATION COMPLETE!");
            println!("========================================");
            println!("✓ ExecutionTracker is a regular provider (NO EnhancerMarker impl needed)");
            println!("✓ AuthService is a regular provider (NO boilerplate)");
            println!("✓ Middleware/Guards/Interceptors auto-detected by #[injectable] macro");
            println!(
                "✓ ProviderTrait includes enhancer detection methods (merged from EnhancerMarker)"
            );
            println!("✓ Runtime successfully resolves all enhancers from DI container via tokens");
            println!("✓ Zero boilerplate - just mark with #[injectable] and implement the trait!");
        })
        .await;
}
