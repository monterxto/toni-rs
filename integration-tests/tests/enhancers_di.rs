mod common;

use std::sync::{Arc, Mutex, OnceLock};
use toni::async_trait;
use toni::enhancer::{guard, interceptor, middleware};
use toni::injector::Context;
use toni::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};
use toni::traits_helpers::{Guard, Interceptor, InterceptorNext, MiddlewareConsumer};
use toni::{
    controller, get, injectable, module, provider_value, use_guards, use_interceptors,
    Body as ToniBody, HttpRequest, RequestPart,
};

use common::TestServer;
use serial_test::serial;

// ---- shared tracker -----------------------------------------------------------
// Tests are serial; all share the same Arc via clone. Each test calls clear()
// before its first request so state from a prior test doesn't leak.

static TRACKER: OnceLock<ExecutionTracker> = OnceLock::new();

fn get_tracker() -> ExecutionTracker {
    TRACKER.get().unwrap().clone()
}

#[derive(Clone)]
pub struct ExecutionTracker {
    log: Arc<Mutex<Vec<String>>>,
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

// ---- injectable service used by enhancers ------------------------------------

#[injectable(pub struct AuthService {
    tracker: ExecutionTracker,
})]
impl AuthService {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }

    pub fn is_admin(&self, req: &RequestPart) -> bool {
        self.tracker.track("service:auth_check");
        req.headers.contains_key("x-admin-token")
    }

    pub fn validate_user(&self, req: &RequestPart) -> bool {
        self.tracker.track("service:user_validation");
        req.headers.contains_key("x-user-id")
    }
}

// ---- DI-registered middleware ------------------------------------------------

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
        if !req.headers().contains_key("x-request-id") {
            let mut response = toni::HttpResponse::new();
            response.status = 400;
            response.body = Some(ToniBody::text("Missing X-Request-ID header".to_string()));
            return Ok(response);
        }
        next.run(req).await
    }
}

// ---- DI-registered guards ----------------------------------------------------

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

// ---- DI-registered interceptors ----------------------------------------------

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

// ---- controller --------------------------------------------------------------

#[controller("/api", pub struct EnhancerTestController {
    tracker: ExecutionTracker,
})]
impl EnhancerTestController {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }

    #[get("/admin")]
    #[use_guards(AdminGuard)]
    #[use_interceptors(LoggingInterceptor)]
    fn admin_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:admin");
        ToniBody::text("Admin access granted".to_string())
    }

    #[get("/user")]
    #[use_guards(UserGuard)]
    #[use_interceptors(TimingInterceptor, LoggingInterceptor)]
    fn user_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:user");
        ToniBody::text("User access granted".to_string())
    }

    #[get("/public")]
    fn public_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:public");
        ToniBody::text("Public access".to_string())
    }
}

// ---- module ------------------------------------------------------------------

#[module(
    controllers: [EnhancerTestController],
    providers: [
        provider_value!(ExecutionTracker, get_tracker()),
        AuthService,
        AdminGuard,
        UserGuard,
        LoggingInterceptor,
        TimingInterceptor,
        RequestTrackingMiddleware,
        HeaderValidationMiddleware,
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

// ---- tests -------------------------------------------------------------------

#[serial]
#[tokio_localset_test::localset_test]
async fn di_guard_with_injected_deps() {
    TRACKER.set(ExecutionTracker::new()).ok();
    let tracker = get_tracker();

    let server = TestServer::start(EnhancerDITestModule::module_definition()).await;

    // guard rejects when auth service says no
    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/admin"))
        .header("X-Request-ID", "test-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
    let log = tracker.get_log();
    assert!(log.contains(&"guard:admin_check".to_string()));
    assert!(log.contains(&"service:auth_check".to_string()));
    assert!(!log.contains(&"controller:admin".to_string()));

    // guard passes when auth service says yes
    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/admin"))
        .header("X-Request-ID", "test-123")
        .header("X-Admin-Token", "valid")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Admin access granted");
    let log = tracker.get_log();
    assert!(log.contains(&"guard:admin_check".to_string()));
    assert!(log.contains(&"service:auth_check".to_string()));
    assert!(log.contains(&"interceptor:before".to_string()));
    assert!(log.contains(&"controller:admin".to_string()));
    assert!(log.contains(&"interceptor:after".to_string()));
}

#[serial]
#[tokio_localset_test::localset_test]
async fn di_interceptor_execution_order() {
    TRACKER.set(ExecutionTracker::new()).ok();
    let tracker = get_tracker();

    let server = TestServer::start(EnhancerDITestModule::module_definition()).await;

    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/user"))
        .header("X-Request-ID", "test-123")
        .header("X-User-Id", "123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let log = tracker.get_log();
    let timing_start = log
        .iter()
        .position(|e| e == "interceptor:timing_start")
        .expect("timing_start not in log");
    let log_before = log
        .iter()
        .position(|e| e == "interceptor:before")
        .expect("interceptor:before not in log");
    let controller_pos = log
        .iter()
        .position(|e| e == "controller:user")
        .expect("controller:user not in log");
    let log_after = log
        .iter()
        .position(|e| e == "interceptor:after")
        .expect("interceptor:after not in log");
    let timing_end = log
        .iter()
        .position(|e| e == "interceptor:timing_end")
        .expect("timing_end not in log");

    assert!(
        timing_start < log_before,
        "TimingInterceptor must wrap LoggingInterceptor (outer runs first)"
    );
    assert!(log_before < controller_pos);
    assert!(controller_pos < log_after);
    assert!(
        log_after < timing_end,
        "TimingInterceptor must close last (outer closes last)"
    );
}

#[serial]
#[tokio_localset_test::localset_test]
async fn di_middleware_with_injected_deps() {
    TRACKER.set(ExecutionTracker::new()).ok();
    let tracker = get_tracker();

    let server = TestServer::start(EnhancerDITestModule::module_definition()).await;

    // middleware rejects when required header is absent
    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/public"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    assert_eq!(resp.text().await.unwrap(), "Missing X-Request-ID header");
    let log = tracker.get_log();
    assert!(log.contains(&"middleware:header_validation".to_string()));
    assert!(!log.contains(&"controller:public".to_string()));

    // middleware passes and adds response header when required header present
    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/public"))
        .header("X-Request-ID", "test-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers().contains_key("x-middleware"),
        "RequestTrackingMiddleware must add X-Middleware response header"
    );
    let log = tracker.get_log();
    assert!(log.contains(&"middleware:request_tracking".to_string()));
    assert!(log.contains(&"middleware:header_validation".to_string()));
    assert!(log.contains(&"controller:public".to_string()));
}
