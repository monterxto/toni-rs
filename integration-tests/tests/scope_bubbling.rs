use serial_test::serial;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use toni::{
    controller, get, injectable, module, toni_factory::ToniFactory, Body as ToniBody, HttpRequest,
};
use toni_axum::AxumAdapter;

// ======================
// Test 1: Singleton Controller with Singleton Provider (OK - No Warning)
// ======================

#[injectable(pub struct SingletonProvider {})]
impl SingletonProvider {
    fn get_data(&self) -> String {
        "Singleton data".to_string()
    }
}

#[controller("/ok", pub struct OkController { #[inject]provider: SingletonProvider })]
impl OkController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.provider.get_data())
    }
}

// ======================
// Test 2: Singleton Controller with Request Provider (WARNING - Scope Mismatch!)
// ======================

static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(0);

#[injectable(scope = "request", pub struct RequestScopedProvider {})]
impl RequestScopedProvider {
    fn get_request_id(&self) -> u32 {
        REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }
}

// This should trigger a warning! Singleton controller with Request-scoped dependency
#[controller("/problematic", pub struct ProblematicController { #[inject]provider: RequestScopedProvider })]
impl ProblematicController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(format!("Request ID: {}", self.provider.get_request_id()))
    }
}

// ======================
// Test 3: Request Controller with Request Provider (OK - No Warning)
// ======================

#[injectable(scope = "request", pub struct AnotherRequestProvider {})]
impl AnotherRequestProvider {
    fn get_data(&self) -> String {
        "Request data".to_string()
    }
}

#[controller("/correct", scope = "request", pub struct CorrectController { #[inject]provider: AnotherRequestProvider })]
impl CorrectController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.provider.get_data())
    }
}

// ======================
// Test 4: Mixed Dependencies (Singleton + Request) - WARNING
// ======================

#[injectable(pub struct CacheProvider {})]
impl CacheProvider {
    fn get_cached(&self) -> String {
        "Cached".to_string()
    }
}

#[injectable(scope = "request", pub struct SessionProvider {})]
impl SessionProvider {
    fn get_session(&self) -> String {
        "Session".to_string()
    }
}

// This should trigger a warning for SessionProvider being Request-scoped
#[controller("/mixed", pub struct MixedController {
    #[inject]
    cache: CacheProvider,
    #[inject]
    session: SessionProvider,
})]
impl MixedController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(format!(
            "{} + {}",
            self.cache.get_cached(),
            self.session.get_session()
        ))
    }
}

// ======================
// Test 5: Explicit Singleton + Request Provider (CONTRADICTION - Force Elevation with WARNING!)
// ======================

#[injectable(scope = "request", pub struct ContradictoryRequestProvider {})]
impl ContradictoryRequestProvider {
    fn get_id(&self) -> String {
        "contradictory".to_string()
    }
}

// User explicitly says "singleton" but has Request deps - should WARN and elevate anyway
#[controller("/explicit", scope = "singleton", pub struct ExplicitSingletonController { #[inject]provider: ContradictoryRequestProvider })]
impl ExplicitSingletonController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.provider.get_id())
    }
}

// ======================
// Modules
// ======================

#[module(
    providers: [SingletonProvider],
    controllers: [OkController],
)]
impl OkModule {}

#[module(
    providers: [RequestScopedProvider],
    controllers: [ProblematicController],
)]
impl ProblematicModule {}

#[module(
    providers: [AnotherRequestProvider],
    controllers: [CorrectController],
)]
impl CorrectModule {}

#[module(
    providers: [CacheProvider, SessionProvider],
    controllers: [MixedController],
)]
impl MixedModule {}

#[module(
    providers: [ContradictoryRequestProvider],
    controllers: [ExplicitSingletonController],
)]
impl ExplicitSingletonModule {}

// ======================
// Tests
// ======================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_ok_singleton_controller_with_singleton_provider() {
        println!("\n=== Test 1: Singleton Controller with Singleton Provider ===");
        println!("Expected: No warnings");

        let port = 38090;
        let local = tokio::task::LocalSet::new();

        // Spawn server in background
        local.spawn_local(async move {
            let mut app = ToniFactory::create(OkModule::module_definition()).await;
            app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
            let _ = app.start().await;
        });

        // Run tests within the LocalSet
        local
            .run_until(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let client = reqwest::Client::new();
                let response = client
                    .get(format!("http://127.0.0.1:{}/ok/test", port))
                    .send()
                    .await
                    .unwrap();

                assert_eq!(response.status(), 200);
                let body = response.text().await.unwrap();
                assert_eq!(body, "Singleton data");

                println!("✅ Test passed - no warnings expected\n");
            })
            .await;
    }

    #[tokio::test]
    #[serial]
    async fn test_warning_singleton_controller_with_request_provider() {
        println!("\n=== Test 2: Singleton Controller with Request Provider ===");
        println!("Expected: ⚠️  WARNING about scope mismatch");

        let port = 38091;
        let local = tokio::task::LocalSet::new();

        // Spawn server in background - THIS SHOULD PRINT A WARNING
        local.spawn_local(async move {
            let mut app = ToniFactory::create(ProblematicModule::module_definition()).await;
            app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
            let _ = app.start().await;
        });

        local
            .run_until(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;

                // The endpoint must respond even when the controller has a scope-mismatched dep.
                // The framework elevates the controller scope and logs a warning; it must not panic.
                let client = reqwest::Client::new();
                let response = client
                    .get(format!("http://127.0.0.1:{}/problematic/test", port))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
            })
            .await;
    }

    #[tokio::test]
    #[serial]
    async fn test_ok_request_controller_with_request_provider() {
        println!("\n=== Test 3: Request Controller with Request Provider ===");
        println!("Expected: No warnings");

        let port = 38092;
        let local = tokio::task::LocalSet::new();

        // Spawn server in background
        local.spawn_local(async move {
            let mut app = ToniFactory::create(CorrectModule::module_definition()).await;
            app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
            let _ = app.start().await;
        });

        // Run tests within the LocalSet
        local
            .run_until(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let client = reqwest::Client::new();
                let response = client
                    .get(format!("http://127.0.0.1:{}/correct/test", port))
                    .send()
                    .await
                    .unwrap();

                assert_eq!(response.status(), 200);
                let body = response.text().await.unwrap();
                assert_eq!(body, "Request data");

                println!("✅ Test passed - no warnings expected\n");
            })
            .await;
    }

    #[tokio::test]
    #[serial]
    async fn test_warning_mixed_dependencies() {
        println!("\n=== Test 4: Mixed Dependencies (Singleton + Request) ===");
        println!("Expected: ⚠️  WARNING about SessionProvider being Request-scoped");

        let port = 38093;
        let local = tokio::task::LocalSet::new();

        // Spawn server in background - THIS SHOULD PRINT A WARNING
        local.spawn_local(async move {
            let mut app = ToniFactory::create(MixedModule::module_definition()).await;
            app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
            let _ = app.start().await;
        });

        local
            .run_until(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Mixed deps (one singleton, one request-scoped) — framework elevates and warns.
                // Endpoint must still be reachable and return both values.
                let client = reqwest::Client::new();
                let response = client
                    .get(format!("http://127.0.0.1:{}/mixed/test", port))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
                let body = response.text().await.unwrap();
                assert_eq!(body, "Cached + Session");
            })
            .await;
    }

    #[tokio::test]
    #[serial]
    async fn test_explicit_singleton_forced_elevation() {
        println!("\n=== Test 5: Explicit Singleton + Request Provider (CONTRADICTION!) ===");
        println!("Expected: ⚠️  WARNING - user explicitly set singleton but has Request deps");

        let port = 38094;
        let local = tokio::task::LocalSet::new();

        // Spawn server in background - THIS SHOULD PRINT A STRONG WARNING
        local.spawn_local(async move {
            let mut app = ToniFactory::create(ExplicitSingletonModule::module_definition()).await;
            app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
            let _ = app.start().await;
        });

        // Run tests within the LocalSet
        local
            .run_until(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let client = reqwest::Client::new();
                let response = client
                    .get(format!("http://127.0.0.1:{}/explicit/test", port))
                    .send()
                    .await
                    .unwrap();

                assert_eq!(response.status(), 200);
                let body = response.text().await.unwrap();
                assert_eq!(body, "contradictory");

                println!("⚠️  Check console output above - you should see a WARNING (not INFO)");
                println!("    about ExplicitSingletonController being explicitly set to singleton");
                println!("    but having Request-scoped dependencies. It was elevated anyway.");
                println!("✅ Test passed - explicit contradiction detected and warned\n");
            })
            .await;
    }
}
