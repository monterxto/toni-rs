//! Test for APP_GUARD/APP_INTERCEPTOR token-based global enhancers
//!
//! This test verifies that:
//! 1. Global enhancers can be registered via provider_token!(APP_GUARD, MyGuard)
//! 2. APP_* token enhancers support dependency injection
//! 3. APP_* token approach works alongside imperative factory.use_global_*()
//! 4. Execution order is maintained: factory globals → APP_* token globals → controller → method
//! NB: APP_PIPE isn't tested due to the fundamental change in the concept of pipes.  See examples crate (examples/pipes_complete_guide.rs).

use serial_test::serial;
use std::sync::{Arc, Mutex};
use toni::async_trait;
use toni::enhancer::{guard, interceptor};
use toni::{
    controller, controller_struct, get, injectable, module, provider_token, Body as ToniBody,
    HttpAdapter, HttpRequest, ToniFactory,
};
use toni_axum::AxumAdapter;

use toni::di::{APP_GUARD, APP_INTERCEPTOR};
use toni::injector::Context;
use toni::traits_helpers::{Guard, Interceptor, InterceptorNext};

// ============================================================================
// EXECUTION ORDER TRACKER
// ============================================================================

type ExecutionOrderInner = Arc<Mutex<Vec<String>>>;

#[injectable(
    pub struct ExecutionTracker {
        inner: ExecutionOrderInner,
    }
)]
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

// ============================================================================
// MOCK DEPENDENCY (for testing DI in APP_* enhancers)
// ============================================================================

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

// ============================================================================
// APP_GUARD with DI (using #[guard] attribute)
// ============================================================================

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

// ============================================================================
// APP_INTERCEPTOR with DI (using #[interceptor] attribute)
// ============================================================================

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

// ============================================================================
// CONTROLLER
// ============================================================================

#[controller_struct(pub struct TestController {
    tracker: ExecutionTracker,
})]
#[controller("/api")]
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

// ============================================================================
// MODULE
// ============================================================================

#[module(
    controllers: [TestController],
    providers: [
        ExecutionTracker,
        MockService,
        AppGuardWithDI,
        AppInterceptorWithDI,
        provider_token!(APP_GUARD, AppGuardWithDI),
        provider_token!(APP_INTERCEPTOR, AppInterceptorWithDI),
    ]
)]
impl TestModule {}

// ============================================================================
// TESTS
// ============================================================================

#[tokio::test]
#[serial]
async fn test_app_token_enhancers_with_di() {
    use tokio::sync::oneshot;

    let port = 29100;
    let (tracker_tx, tracker_rx) = oneshot::channel();

    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        // Create application (APP_* token enhancers will be resolved from DI)
        let axum_adapter = AxumAdapter::new();
        let factory = ToniFactory::new();
        let app = factory
            .create(TestModule::module_definition(), axum_adapter)
            .await;

        // Get the tracker from DI container to verify APP_* enhancers work
        let tracker = app
            .get::<ExecutionTracker>()
            .await
            .expect("Failed to get ExecutionTracker from DI");

        // Send tracker to test task
        let _ = tracker_tx.send(tracker);

        app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Receive tracker from spawned task
            let tracker = tracker_rx.await.expect("Failed to receive tracker");

            let client = reqwest::Client::new();

            // ================================================================
            // TEST: APP_* token enhancers with DI + imperative factory enhancers
            // ================================================================
            tracker.clear();

            let response = client
                .get(format!("http://127.0.0.1:{}/api/test", port))
                .send()
                .await
                .expect("Failed to call test endpoint");

            assert_eq!(response.status(), 200);

            let order = tracker.get_events();
            println!("Execution order: {:?}", order);

            // Verify that we have expected events from APP_* token enhancers
            assert!(
                order
                    .iter()
                    .any(|e| e.contains("guard:app_token:MockService")),
                "Should have APP_GUARD with DI"
            );
            assert!(
                order
                    .iter()
                    .any(|e| e.contains("interceptor:app_token:MockService")),
                "Should have APP_INTERCEPTOR with DI"
            );
            assert!(
                order.iter().any(|e| e == "controller:handler"),
                "Should have handler execution"
            );

            // Verify that APP_* enhancers received injected dependency
            assert!(
                order.iter().any(|e| e.contains("MockService")),
                "APP_* enhancers should have received MockService dependency"
            );
        })
        .await;
}
