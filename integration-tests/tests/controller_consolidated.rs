mod common;

use common::TestServer;
use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody, HttpRequest};

#[serial]
#[tokio_localset_test::localset_test]
async fn consolidated_controller_basic() {
    #[injectable(pub struct TestService {})]
    impl TestService {
        pub fn message(&self) -> String {
            "consolidated".to_string()
        }
    }

    #[controller(pub struct TestController {
        #[inject]
        service: TestService,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.service.message())
        }
    }

    #[module(
        providers: [TestService],
        controllers: [TestController],
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
    assert_eq!(body, "consolidated");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn consolidated_controller_with_path() {
    #[injectable(pub struct PathService {})]
    impl PathService {
        pub fn data(&self) -> String {
            "path-data".to_string()
        }
    }

    #[controller("/api", pub struct PathController {
        #[inject]
        service: PathService,
    })]
    impl PathController {
        #[get("/data")]
        fn get_data(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.service.data())
        }
    }

    #[module(
        providers: [PathService],
        controllers: [PathController],
    )]
    impl PathModule {}

    let server = TestServer::start(PathModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/data"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "path-data");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn consolidated_controller_with_scope() {
    #[injectable(pub struct ScopeService {})]
    impl ScopeService {
        pub fn scope_info(&self) -> String {
            "request-scope".to_string()
        }
    }

    #[controller("/scoped", scope = "request", pub struct ScopeController {
        #[inject]
        service: ScopeService,
    })]
    impl ScopeController {
        #[get("/info")]
        fn info(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.service.scope_info())
        }
    }

    #[module(
        providers: [ScopeService],
        controllers: [ScopeController],
    )]
    impl ScopeModule {}

    let server = TestServer::start(ScopeModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/scoped/info"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "request-scope");
}
