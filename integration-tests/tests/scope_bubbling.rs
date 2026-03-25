mod common;

use common::TestServer;
use serial_test::serial;
use std::sync::atomic::{AtomicU32, Ordering};
use toni::{
    controller, get, injectable, module, toni_factory::ToniFactory, Body as ToniBody, HttpRequest,
};

// ---- Test 1: Singleton controller + singleton provider ----------------------

#[injectable(pub struct SingletonProvider {})]
impl SingletonProvider {
    fn get_data(&self) -> String {
        "Singleton data".to_string()
    }
}

#[controller("/ok", pub struct OkController { #[inject] provider: SingletonProvider })]
impl OkController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(self.provider.get_data())
    }
}

#[module(providers: [SingletonProvider], controllers: [OkController])]
impl OkModule {}

// ---- Test 2: Singleton controller + request-scoped provider (scope promotion) ---

static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(0);

#[injectable(scope = "request", pub struct RequestScopedProvider {})]
impl RequestScopedProvider {
    fn get_request_id(&self) -> u32 {
        REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }
}

// Singleton controller with a request-scoped dep — framework promotes to request scope
#[controller("/problematic", pub struct ProblematicController { #[inject] provider: RequestScopedProvider })]
impl ProblematicController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(format!("Request ID: {}", self.provider.get_request_id()))
    }
}

#[module(providers: [RequestScopedProvider], controllers: [ProblematicController])]
impl ProblematicModule {}

// ---- Test 3: Request controller + request provider (valid) ------------------

#[injectable(scope = "request", pub struct AnotherRequestProvider {})]
impl AnotherRequestProvider {
    fn get_data(&self) -> String {
        "Request data".to_string()
    }
}

#[controller("/correct", scope = "request", pub struct CorrectController { #[inject] provider: AnotherRequestProvider })]
impl CorrectController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(self.provider.get_data())
    }
}

#[module(providers: [AnotherRequestProvider], controllers: [CorrectController])]
impl CorrectModule {}

// ---- Test 4: Mixed singleton + request deps (scope promotion) ---------------

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

#[controller("/mixed", pub struct MixedController {
    #[inject] cache: CacheProvider,
    #[inject] session: SessionProvider,
})]
impl MixedController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(format!(
            "{} + {}",
            self.cache.get_cached(),
            self.session.get_session()
        ))
    }
}

#[module(providers: [CacheProvider, SessionProvider], controllers: [MixedController])]
impl MixedModule {}

// ---- Test 5: Explicit singleton + request dep (contradiction, still promotes) -----

#[injectable(scope = "request", pub struct ContradictoryRequestProvider {})]
impl ContradictoryRequestProvider {
    fn get_id(&self) -> String {
        "contradictory".to_string()
    }
}

#[controller("/explicit", scope = "singleton", pub struct ExplicitSingletonController { #[inject] provider: ContradictoryRequestProvider })]
impl ExplicitSingletonController {
    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(self.provider.get_id())
    }
}

#[module(providers: [ContradictoryRequestProvider], controllers: [ExplicitSingletonController])]
impl ExplicitSingletonModule {}

// ---- tests ------------------------------------------------------------------

#[serial]
#[tokio_localset_test::localset_test]
async fn singleton_controller_with_singleton_provider() {
    let server = TestServer::start(OkModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/ok/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Singleton data");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn singleton_controller_promoted_to_request_scope_when_dep_is_request() {
    // The framework detects the scope mismatch and silently promotes the controller to
    // request-scoped rather than panicking. The endpoint must still be reachable and
    // return the request-scoped provider's output.
    let server = TestServer::start(ProblematicModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/problematic/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(
        resp.text().await.unwrap().starts_with("Request ID:"),
        "request-scoped provider must be accessible after scope promotion"
    );
}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_controller_with_request_provider() {
    let server = TestServer::start(CorrectModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/correct/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Request data");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn mixed_scope_deps_promote_controller_to_request() {
    let server = TestServer::start(MixedModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/mixed/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Cached + Session");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn explicit_singleton_with_request_dep_still_promotes() {
    let server = TestServer::start(ExplicitSingletonModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/explicit/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "contradictory");
}
