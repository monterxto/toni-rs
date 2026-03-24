mod common;

use std::sync::{Arc, Mutex, OnceLock};
use toni::async_trait;
use toni::enhancer::{guard, interceptor};
use toni::injector::Context;
use toni::traits_helpers::{Guard, Interceptor, InterceptorNext};
use toni::{
    controller, get, injectable, module, provider_token, provider_value, Body as ToniBody,
    HttpRequest,
};
use toni::di::{APP_GUARD, APP_INTERCEPTOR};

use common::TestServer;
use serial_test::serial;

static TRACKER: OnceLock<ExecutionTracker> = OnceLock::new();

fn get_tracker() -> ExecutionTracker {
    TRACKER.get().unwrap().clone()
}

#[derive(Clone)]
pub struct ExecutionTracker {
    inner: Arc<Mutex<Vec<String>>>,
}

impl ExecutionTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn track(&self, event: &str) {
        self.inner.lock().unwrap().push(event.to_string());
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    pub fn get_events(&self) -> Vec<String> {
        self.inner.lock().unwrap().clone()
    }
}

#[injectable(
    pub struct MockService {
        name: String,
    }
)]
impl MockService {
    pub fn new() -> Self {
        Self {
            name: "MockService".to_string(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
}

#[injectable(pub struct AppGuardWithDI {
    service: MockService,
    tracker: ExecutionTracker,
})]
#[guard]
impl AppGuardWithDI {
    pub fn new(service: MockService, tracker: ExecutionTracker) -> Self {
        Self { service, tracker }
    }
}

impl Guard for AppGuardWithDI {
    fn can_activate(&self, _context: &Context) -> bool {
        self.tracker
            .track(&format!("guard:app_token:{}", self.service.get_name()));
        true
    }
}

#[injectable(pub struct AppInterceptorWithDI {
    service: MockService,
    tracker: ExecutionTracker,
})]
#[interceptor]
impl AppInterceptorWithDI {
    pub fn new(service: MockService, tracker: ExecutionTracker) -> Self {
        Self { service, tracker }
    }
}

#[async_trait]
impl Interceptor for AppInterceptorWithDI {
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
        self.tracker.track(&format!(
            "interceptor:app_token:{}:before",
            self.service.get_name()
        ));
        next.run(context).await;
        self.tracker.track(&format!(
            "interceptor:app_token:{}:after",
            self.service.get_name()
        ));
    }
}

#[controller("/api", pub struct TestController {
    tracker: ExecutionTracker,
})]
impl TestController {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }

    #[get("/test")]
    fn test_endpoint(&self, _req: HttpRequest) -> ToniBody {
        self.tracker.track("controller:handler");
        ToniBody::Text("OK".to_string())
    }
}

#[module(
    controllers: [TestController],
    providers: [
        provider_value!(ExecutionTracker, get_tracker()),
        MockService,
        AppGuardWithDI,
        AppInterceptorWithDI,
        provider_token!(APP_GUARD, AppGuardWithDI),
        provider_token!(APP_INTERCEPTOR, AppInterceptorWithDI),
    ]
)]
impl TestModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn app_token_enhancers_with_di() {
    TRACKER.set(ExecutionTracker::new()).ok();
    let tracker = get_tracker();

    let server = TestServer::start(TestModule::module_definition()).await;

    tracker.clear();
    let resp = server
        .client()
        .get(server.url("/api/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let events = tracker.get_events();
    assert!(
        events.iter().any(|e| e.contains("guard:app_token:MockService")),
        "APP_GUARD must run and its injected MockService must be accessible"
    );
    assert!(
        events.iter().any(|e| e.contains("interceptor:app_token:MockService:before")),
        "APP_INTERCEPTOR before must run"
    );
    assert!(
        events.iter().any(|e| e == "controller:handler"),
        "controller must run after guards and before interceptor after"
    );
    assert!(
        events.iter().any(|e| e.contains("interceptor:app_token:MockService:after")),
        "APP_INTERCEPTOR after must run"
    );

    // guard runs before controller
    let guard_pos = events.iter().position(|e| e.contains("guard:app_token")).unwrap();
    let ctrl_pos = events.iter().position(|e| e == "controller:handler").unwrap();
    assert!(guard_pos < ctrl_pos);
}
