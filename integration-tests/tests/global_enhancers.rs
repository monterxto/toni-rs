//! Test for three-level enhancer hierarchy: global < controller < method
//!
//! This test verifies that:
//! 1. Global enhancers are registered via ToniFactory
//! 2. Controller-level enhancers apply to all methods
//! 3. Method-level enhancers add to controller-level
//! 4. Execution order is: global → controller → method
//! 5. Same enhancer can be registered multiple times at different levels

use serial_test::serial;
use std::sync::{Arc, Mutex};
use toni::async_trait;
use toni::{
    controller, get, module, use_guards, use_interceptors, use_pipes, Body as ToniBody,
    HttpAdapter, HttpRequest, ToniFactory,
};
use toni_axum::AxumAdapter;

use toni::injector::Context;
use toni::traits_helpers::{Guard, Interceptor, InterceptorNext, Pipe};

// ============================================================================
// EXECUTION ORDER TRACKER
// ============================================================================

type ExecutionOrderInner = Arc<Mutex<Vec<String>>>;

#[derive(Clone)]
pub struct ExecutionOrder {
    inner: ExecutionOrderInner,
}

impl ExecutionOrder {
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

// Global tracker for testing
static mut GLOBAL_TRACKER: Option<ExecutionOrder> = None;

fn init_tracker() -> ExecutionOrder {
    let tracker = ExecutionOrder::new();
    unsafe {
        GLOBAL_TRACKER = Some(tracker.clone());
    }
    tracker
}

fn get_tracker() -> ExecutionOrder {
    unsafe { GLOBAL_TRACKER.clone().expect("Tracker not initialized") }
}

// ============================================================================
// GUARD IMPLEMENTATIONS
// ============================================================================

pub struct GlobalGuard;

impl GlobalGuard {
    pub fn new() -> Self {
        Self
    }
}

impl Guard for GlobalGuard {
    fn can_activate(&self, _context: &Context) -> bool {
        get_tracker().track("guard:global");
        true
    }
}

pub struct ControllerGuard;

impl ControllerGuard {
    pub fn new() -> Self {
        Self
    }
}

impl Guard for ControllerGuard {
    fn can_activate(&self, _context: &Context) -> bool {
        get_tracker().track("guard:controller");
        true
    }
}

pub struct MethodGuard;

impl MethodGuard {
    pub fn new() -> Self {
        Self
    }
}

impl Guard for MethodGuard {
    fn can_activate(&self, _context: &Context) -> bool {
        get_tracker().track("guard:method");
        true
    }
}

// ============================================================================
// INTERCEPTOR IMPLEMENTATIONS
// ============================================================================

pub struct GlobalInterceptor;

impl GlobalInterceptor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Interceptor for GlobalInterceptor {
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
        get_tracker().track("interceptor:global:before");
        next.run(context).await;
        get_tracker().track("interceptor:global:after");
    }
}

pub struct ControllerInterceptor;

impl ControllerInterceptor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Interceptor for ControllerInterceptor {
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
        get_tracker().track("interceptor:controller:before");
        next.run(context).await;
        get_tracker().track("interceptor:controller:after");
    }
}

pub struct MethodInterceptor;

impl MethodInterceptor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Interceptor for MethodInterceptor {
    async fn intercept(&self, context: &mut Context, next: Box<dyn InterceptorNext>) {
        get_tracker().track("interceptor:method:before");
        next.run(context).await;
        get_tracker().track("interceptor:method:after");
    }
}

// ============================================================================
// PIPE IMPLEMENTATIONS
// ============================================================================

pub struct GlobalPipe;

impl GlobalPipe {
    pub fn new() -> Self {
        Self
    }
}

impl Pipe for GlobalPipe {
    fn process(&self, _context: &mut Context) {
        get_tracker().track("pipe:global");
    }
}

pub struct ControllerPipe;

impl ControllerPipe {
    pub fn new() -> Self {
        Self
    }
}

impl Pipe for ControllerPipe {
    fn process(&self, _context: &mut Context) {
        get_tracker().track("pipe:controller");
    }
}

pub struct MethodPipe;

impl MethodPipe {
    pub fn new() -> Self {
        Self
    }
}

impl Pipe for MethodPipe {
    fn process(&self, _context: &mut Context) {
        get_tracker().track("pipe:method");
    }
}

// ============================================================================
// CONTROLLER WITH THREE-LEVEL ENHANCERS
// ============================================================================

#[controller(
    "/api",
    pub struct TestController {}
)]
#[use_guards(ControllerGuard{})]
#[use_interceptors(ControllerInterceptor{})]
#[use_pipes(ControllerPipe{})]
impl TestController {
    /// Endpoint with all three levels:
    /// - Global (from ToniFactory)
    /// - Controller (from impl block)
    /// - Method (from this method)
    #[use_guards(MethodGuard{})]
    #[use_interceptors(MethodInterceptor{})]
    #[use_pipes(MethodPipe{})]
    #[get("/three-level")]
    fn three_level_endpoint(&self, _req: HttpRequest) -> ToniBody {
        get_tracker().track("controller:three_level");
        ToniBody::Text("Three-level test".to_string())
    }

    /// Endpoint with only global + controller levels (no method-level)
    #[get("/two-level")]
    fn two_level_endpoint(&self, _req: HttpRequest) -> ToniBody {
        get_tracker().track("controller:two_level");
        ToniBody::Text("Two-level test".to_string())
    }

    /// Endpoint with duplicated guard at all three levels
    #[use_guards(GlobalGuard{})]
    #[get("/duplicate")]
    fn duplicate_endpoint(&self, _req: HttpRequest) -> ToniBody {
        get_tracker().track("controller:duplicate");
        ToniBody::Text("Duplicate test".to_string())
    }
}

#[module(
    controllers: [TestController]
)]
impl TestModule {}

// ============================================================================
// TESTS
// ============================================================================

#[tokio::test]
#[serial]
async fn test_three_level_enhancer_hierarchy() {
    let tracker = init_tracker();
    let port = 29095;

    let local = tokio::task::LocalSet::new();

    local.spawn_local(async move {
        // Create factory and register GLOBAL enhancers
        let mut factory = ToniFactory::new();
        factory
            .use_global_guards(Arc::new(GlobalGuard::new()))
            .use_global_interceptors(Arc::new(GlobalInterceptor::new()))
            .use_global_pipes(Arc::new(GlobalPipe::new()));

        // Create application
        let axum_adapter = AxumAdapter::new();
        let app = ToniFactory::create(TestModule::module_definition(), axum_adapter).await;

        app.listen(port, "127.0.0.1").await;
    });

    local
        .run_until(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // ================================================================
            // TEST 1: Three-level hierarchy (global + controller + method)
            // ================================================================
            tracker.clear();

            let response = client
                .get(format!("http://127.0.0.1:{}/api/three-level", port))
                .send()
                .await
                .expect("Failed to call three-level endpoint");

            assert_eq!(response.status(), 200);

            let order = tracker.get_events();
            println!("Three-level execution order: {:?}", order);

            // Verify execution order: global → controller → method
            // Guards execute in order
            assert_eq!(order[0], "guard:global");
            assert_eq!(order[1], "guard:controller");
            assert_eq!(order[2], "guard:method");

            // Interceptors execute: global:before → controller:before → method:before → handler → method:after → controller:after → global:after
            assert_eq!(order[3], "interceptor:global:before");
            assert_eq!(order[4], "interceptor:controller:before");
            assert_eq!(order[5], "interceptor:method:before");

            // Pipes execute in order
            assert_eq!(order[6], "pipe:global");
            assert_eq!(order[7], "pipe:controller");
            assert_eq!(order[8], "pipe:method");

            // Controller
            assert_eq!(order[9], "controller:three_level");

            // Interceptors after (reverse order)
            assert_eq!(order[10], "interceptor:method:after");
            assert_eq!(order[11], "interceptor:controller:after");
            assert_eq!(order[12], "interceptor:global:after");

            // ================================================================
            // TEST 2: Two-level hierarchy (global + controller only)
            // ================================================================
            tracker.clear();

            let response = client
                .get(format!("http://127.0.0.1:{}/api/two-level", port))
                .send()
                .await
                .expect("Failed to call two-level endpoint");

            assert_eq!(response.status(), 200);

            let order = tracker.get_events();
            println!("Two-level execution order: {:?}", order);

            // Should only have global and controller enhancers, no method-level
            assert_eq!(order[0], "guard:global");
            assert_eq!(order[1], "guard:controller");
            assert_eq!(order[2], "interceptor:global:before");
            assert_eq!(order[3], "interceptor:controller:before");
            assert_eq!(order[4], "pipe:global");
            assert_eq!(order[5], "pipe:controller");
            assert_eq!(order[6], "controller:two_level");
            assert_eq!(order[7], "interceptor:controller:after");
            assert_eq!(order[8], "interceptor:global:after");

            // ================================================================
            // TEST 3: Duplicate enhancers (GlobalGuard appears twice)
            // ================================================================
            tracker.clear();

            let response = client
                .get(format!("http://127.0.0.1:{}/api/duplicate", port))
                .send()
                .await
                .expect("Failed to call duplicate endpoint");

            assert_eq!(response.status(), 200);

            let order = tracker.get_events();
            println!("Duplicate execution order: {:?}", order);

            // GlobalGuard should execute TWICE: once from global, once from method
            let global_guard_count = order.iter().filter(|e| *e == "guard:global").count();
            assert_eq!(global_guard_count, 2, "GlobalGuard should execute twice");

            // Verify order: global (factory) → controller → method (also global)
            assert_eq!(order[0], "guard:global"); // From factory
            assert_eq!(order[1], "guard:controller"); // From controller
            assert_eq!(order[2], "guard:global"); // From method (duplicate)
        })
        .await;
}
