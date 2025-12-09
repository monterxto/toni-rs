mod common;

use common::{ExecutionOrder, TestServer};
use serial_test::serial;
use toni::async_trait;
use toni::injector::Context;
use toni::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};
use toni::traits_helpers::{Guard, Interceptor, InterceptorNext, MiddlewareConsumer, Pipe};
use toni::{
    controller, controller_struct, get, injectable, module, post, provider_factory, provider_token,
    provider_value, use_guards, use_interceptors, use_pipes, Body as ToniBody, HttpRequest,
    HttpResponse,
};

pub struct OrderTrackerMiddleware {
    name: String,
    tracker: ExecutionOrder,
}

impl OrderTrackerMiddleware {
    pub fn new(name: &str, tracker: ExecutionOrder) -> Self {
        Self {
            name: name.to_string(),
            tracker,
        }
    }
}

#[async_trait]
impl Middleware for OrderTrackerMiddleware {
    async fn handle(&self, mut req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        self.tracker.track(format!("middleware:{}", self.name));
        req.headers_mut()
            .push(("X-Middleware-Order".to_string(), self.name.clone()));
        let mut response = next.run(req).await?;
        response.headers.push((
            "X-Middleware-Modified".to_string(),
            format!("processed-by-{}", self.name),
        ));
        Ok(response)
    }
}

pub struct HeaderCheckMiddleware {
    required_header: String,
    tracker: ExecutionOrder,
}

impl HeaderCheckMiddleware {
    pub fn new(required_header: &str, tracker: ExecutionOrder) -> Self {
        Self {
            required_header: required_header.to_string(),
            tracker,
        }
    }
}

#[async_trait]
impl Middleware for HeaderCheckMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        self.tracker.track("middleware:header_check");
        if !req.has_header(&self.required_header) {
            let mut response = HttpResponse::new();
            response.status = 400;
            response.body = Some(ToniBody::Text(format!(
                "Missing required header: {}",
                self.required_header
            )));
            return Ok(response);
        }
        next.run(req).await
    }
}

pub struct AdminGuard {
    tracker: ExecutionOrder,
}

impl AdminGuard {
    pub fn new(tracker: ExecutionOrder) -> Self {
        Self { tracker }
    }
}

impl Guard for AdminGuard {
    fn can_activate(&self, context: &Context) -> bool {
        self.tracker.track("guard:admin");
        let req = context.take_request();
        req.header("X-Admin-Token")
            .map(|value| value == "secret123")
            .unwrap_or(false)
    }
}

pub struct AuthGuard {
    tracker: ExecutionOrder,
}

impl AuthGuard {
    pub fn new(tracker: ExecutionOrder) -> Self {
        Self { tracker }
    }
}

impl Guard for AuthGuard {
    fn can_activate(&self, context: &Context) -> bool {
        self.tracker.track("guard:auth");
        let req = context.take_request();
        req.has_header("Authorization")
    }
}

pub struct LoggingInterceptor {
    name: String,
    tracker: ExecutionOrder,
}

impl LoggingInterceptor {
    pub fn new(name: &str, tracker: ExecutionOrder) -> Self {
        Self {
            name: name.to_string(),
            tracker,
        }
    }
}

#[async_trait]
impl Interceptor for LoggingInterceptor {
    async fn intercept(&self, _context: &mut Context, next: Box<dyn InterceptorNext>) {
        self.tracker
            .track(format!("interceptor:{}:before", self.name));
        next.run(_context).await;
        self.tracker
            .track(format!("interceptor:{}:after", self.name));
    }
}

pub struct ValidationPipe {
    tracker: ExecutionOrder,
}

impl ValidationPipe {
    pub fn new(tracker: ExecutionOrder) -> Self {
        Self { tracker }
    }
}

impl Pipe for ValidationPipe {
    fn process(&self, context: &mut Context) {
        self.tracker.track("pipe:validation");
        let req = context.take_request();
        let is_invalid = req
            .header("X-Valid")
            .map(|value| value == "false")
            .unwrap_or(false);

        if is_invalid {
            let mut response = HttpResponse::new();
            response.status = 400;
            response.body = Some(ToniBody::Text("Validation failed".to_string()));
            context.set_response(Box::new(response));
            context.abort();
        }
    }
}

pub struct TransformPipe {
    tracker: ExecutionOrder,
}

impl TransformPipe {
    pub fn new(tracker: ExecutionOrder) -> Self {
        Self { tracker }
    }
}

impl Pipe for TransformPipe {
    fn process(&self, _context: &mut Context) {
        self.tracker.track("pipe:transform");
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn enhancers_execution_order() {
    use std::sync::OnceLock;
    static TRACKER: OnceLock<ExecutionOrder> = OnceLock::new();

    let tracker = ExecutionOrder::new();
    TRACKER.set(tracker.clone()).ok();

    fn get_tracker() -> ExecutionOrder {
        TRACKER.get().unwrap().clone()
    }

    #[injectable(pub struct TestService {
        #[inject]
        tracker: ExecutionOrder,
    })]
    impl TestService {
        pub fn process(&self, message: &str) -> String {
            self.tracker.track("service:process");
            format!("Processed: {}", message)
        }
    }

    #[controller_struct(pub struct EnhancerController {
        #[inject]
        service: TestService,
        #[inject]
        tracker: ExecutionOrder,
    })]
    #[use_interceptors(LoggingInterceptor::new("controller", get_tracker()))]
    #[controller("/api")]
    impl EnhancerController {
        #[use_guards(AdminGuard::new(get_tracker()))]
        #[use_interceptors(LoggingInterceptor::new("method", get_tracker()))]
        #[use_pipes(ValidationPipe::new(get_tracker()), TransformPipe::new(get_tracker()))]
        #[get("/protected")]
        fn protected_endpoint(&self, _req: HttpRequest) -> ToniBody {
            self.tracker.track("controller:protected");
            ToniBody::Text("Protected resource".to_string())
        }

        #[use_guards(AuthGuard::new(get_tracker()))]
        #[get("/auth-only")]
        fn auth_only_endpoint(&self, _req: HttpRequest) -> ToniBody {
            self.tracker.track("controller:auth_only");
            ToniBody::Text("Authenticated resource".to_string())
        }

        #[use_interceptors(LoggingInterceptor::new("validate", get_tracker()))]
        #[use_pipes(ValidationPipe::new(get_tracker()), TransformPipe::new(get_tracker()))]
        #[post("/validate")]
        fn validate_endpoint(&self, _req: HttpRequest) -> ToniBody {
            self.tracker.track("controller:validate");
            let result = self.service.process("data");
            ToniBody::Text(result)
        }

        #[get("/public")]
        fn public_endpoint(&self, _req: HttpRequest) -> ToniBody {
            self.tracker.track("controller:public");
            ToniBody::Text("Public resource".to_string())
        }
    }

    #[module(
        controllers: [EnhancerController],
        providers: [
            TestService,
            provider_value!(ExecutionOrder, get_tracker()),
        ],
    )]
    impl EnhancerModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(OrderTrackerMiddleware::new("first", get_tracker()))
                .for_routes(vec!["/api/*"]);
            consumer
                .apply(HeaderCheckMiddleware::new("X-Request-ID", get_tracker()))
                .for_routes(vec!["/api/validate"]);
        }
    }

    let server = TestServer::start(EnhancerModule::module_definition()).await;

    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/public"))
        .header("X-Request-ID", "test-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    tracker.assert_contains("middleware:first");
    tracker.assert_contains("controller:public");

    tracker.clear();
    let resp = server
        .client()
        .post(server.url("/api/validate"))
        .header("X-Request-ID", "test-456")
        .header("X-Valid", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    tracker.assert_contains("middleware:first");
    tracker.assert_contains("middleware:header_check");
    tracker.assert_contains("controller:validate");
    tracker.assert_contains("service:process");

    tracker.clear();
    let resp = server
        .client()
        .post(server.url("/api/validate"))
        .header("X-Request-ID", "test-789")
        .header("X-Valid", "false")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    tracker.assert_not_contains("controller:validate");
    tracker.assert_not_contains("service:process");

    tracker.clear();
    let resp = server
        .client()
        .post(server.url("/api/validate"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    tracker.assert_not_contains("controller:validate");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn guard_authorization() {
    use std::sync::OnceLock;

    static TRACKER: OnceLock<ExecutionOrder> = OnceLock::new();
    let tracker = ExecutionOrder::new();
    TRACKER.set(tracker.clone()).ok();

    fn get_tracker() -> ExecutionOrder {
        TRACKER.get().unwrap().clone()
    }

    #[controller_struct(pub struct TestController {
        #[inject]
        tracker: ExecutionOrder,
    })]
    #[controller("/api")]
    impl TestController {
        #[use_guards(AuthGuard)]
        #[get("/auth-only")]
        fn auth_only(&self, _req: HttpRequest) -> ToniBody {
            self.tracker.track("controller:auth_only");
            ToniBody::Text("Authenticated resource".to_string())
        }
    }

    #[module(
        controllers: [TestController],
        providers: [
            provider_value!(ExecutionOrder, get_tracker()),
            provider_factory!(AuthGuard, |tracker: ExecutionOrder| AuthGuard::new(tracker), guard),
        ],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;

    tracker.clear();
    let _resp = server
        .client()
        .get(server.url("/api/auth-only"))
        .send()
        .await
        .unwrap();
    let events = tracker.events();
    assert!(events.contains(&"guard:auth".to_string()));

    tracker.clear();
    let _resp = server
        .client()
        .get(server.url("/api/auth-only"))
        .header("Authorization", "Bearer valid-token")
        .send()
        .await
        .unwrap();
    let events = tracker.events();
    assert!(events.contains(&"guard:auth".to_string()));
}

#[serial]
#[tokio_localset_test::localset_test]
async fn di_in_enhancers() {
    #[injectable(pub struct AuthService {})]
    impl AuthService {
        pub fn validate(&self, token: &str) -> bool {
            token == "valid"
        }
    }

    struct DIGuard {
        auth: AuthService,
    }

    impl DIGuard {
        pub fn new(auth: AuthService) -> Self {
            Self { auth }
        }
    }

    impl Guard for DIGuard {
        fn can_activate(&self, context: &Context) -> bool {
            let req = context.take_request();
            req.header("X-Token")
                .map(|token| self.auth.validate(token))
                .unwrap_or(false)
        }
    }

    #[controller_struct(pub struct TestController {})]
    #[controller("/api")]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [AuthService],
        controllers: [TestController],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn app_token_global_enhancers() {
    use std::sync::OnceLock;
    use toni::di::APP_GUARD;

    static TRACKER: OnceLock<ExecutionOrder> = OnceLock::new();

    let tracker = ExecutionOrder::new();
    TRACKER.set(tracker.clone()).ok();

    fn get_tracker() -> ExecutionOrder {
        TRACKER.get().unwrap().clone()
    }

    #[injectable(pub struct GlobalGuard {
        #[inject]
        tracker: ExecutionOrder,
    })]
    impl Guard for GlobalGuard {
        fn can_activate(&self, _context: &Context) -> bool {
            self.tracker.track("global_guard");
            true
        }
    }

    #[controller_struct(pub struct TestController {
        #[inject]
        tracker: ExecutionOrder,
    })]
    #[controller("/api")]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            self.tracker.track("controller:test");
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [
            provider_value!(ExecutionOrder, get_tracker()),
            GlobalGuard,
            provider_token!(APP_GUARD, GlobalGuard),
        ],
        controllers: [TestController],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify the global guard was executed
    tracker.assert_contains("global_guard");
    tracker.assert_contains("controller:test");
}
