mod common;

use common::TestServer;
use serial_test::serial;
use std::sync::atomic::{AtomicU32, Ordering};
use toni::{controller, get, injectable, module, Body as ToniBody, HttpRequest};
use toni_config::{Config, ConfigModule, ConfigService};

#[derive(Config, Clone)]
struct TestConfig {
    #[default("test_value".to_string())]
    pub value: String,
}

static SINGLETON_COUNTER: AtomicU32 = AtomicU32::new(0);

#[injectable(pub struct SingletonService {})]
impl SingletonService {
    pub fn new() -> Self {
        SINGLETON_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {}
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn singleton_providers_created_once_across_requests() {
    SINGLETON_COUNTER.store(0, Ordering::SeqCst);

    #[controller("/", pub struct TestController { #[inject] service: SingletonService })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(format!("{}", SINGLETON_COUNTER.load(Ordering::SeqCst)))
        }
    }

    #[module(providers: [SingletonService], controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;

    for _ in 0..5 {
        let resp = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();
        let body = resp.text().await.unwrap();
        assert_eq!(body, "1");
    }
}

static TRANSIENT_COUNTER: AtomicU32 = AtomicU32::new(0);

#[injectable(scope = "transient", pub struct TransientService { id: u32 })]
impl TransientService {
    pub fn new() -> Self {
        let id = TRANSIENT_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self { id }
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn transient_providers_create_unique_instances_per_injection() {
    TRANSIENT_COUNTER.store(0, Ordering::SeqCst);

    #[injectable(pub struct MultiService {
        #[inject] t1: TransientService,
        #[inject] t2: TransientService,
    })]
    impl MultiService {
        pub fn ids(&self) -> (u32, u32) {
            (self.t1.id(), self.t2.id())
        }
    }

    #[controller("/", pub struct TestController { #[inject] service: MultiService })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            let (id1, id2) = self.service.ids();
            ToniBody::Text(format!("{}|{}", id1, id2))
        }
    }

    #[module(providers: [TransientService, MultiService], controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();

    let parts: Vec<&str> = body.split('|').collect();
    assert_eq!(
        parts.len(),
        2,
        "Expected 2 parts separated by '|', got: {}",
        body
    );
    assert_ne!(parts[0], parts[1]);
}



#[serial]
#[tokio_localset_test::localset_test]
async fn field_injection_with_inject_attribute() {
    #[injectable(pub struct DependencyService {})]
    impl DependencyService {
        pub fn value(&self) -> i32 {
            42
        }
    }

    #[injectable(pub struct ServiceWithDeps {
        #[inject]
        dep: DependencyService,
    })]
    impl ServiceWithDeps {
        pub fn get_value(&self) -> i32 {
            self.dep.value()
        }
    }

    #[controller("/", pub struct TestController { #[inject] service: ServiceWithDeps })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(format!("{}", self.service.get_value()))
        }
    }

    #[module(providers: [DependencyService, ServiceWithDeps], controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "42");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn field_injection_with_default_fallback() {
    #[injectable(pub struct ServiceWithDefault {
        #[default(100)]
        value: i32,
    })]
    impl ServiceWithDefault {
        pub fn get_value(&self) -> i32 {
            self.value
        }
    }

    #[controller("/", pub struct TestController { #[inject] service: ServiceWithDefault })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(format!("{}", self.service.get_value()))
        }
    }

    #[module(providers: [ServiceWithDefault], controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "100");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn config_service_injection_in_providers() {
    #[injectable(pub struct ServiceWithConfig {
        #[inject]
        config: ConfigService<TestConfig>,
    })]
    impl ServiceWithConfig {
        pub fn get_value(&self) -> String {
            self.config.get_ref().value.clone()
        }
    }

    #[controller("/", pub struct TestController { #[inject] service: ServiceWithConfig })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.service.get_value())
        }
    }

    #[module(
        imports: [ConfigModule::<TestConfig>::new()],
        providers: [ServiceWithConfig],
        controllers: [TestController]
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "test_value");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn new_attribute_syntax() {
    #[injectable]
    pub struct NewSyntaxService {}

    impl NewSyntaxService {
        pub fn new() -> Self {
            Self {}
        }
    }

    #[controller("/", pub struct TestController { #[inject] service: NewSyntaxService })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(providers: [NewSyntaxService], controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

