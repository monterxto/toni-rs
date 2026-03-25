mod common;

use common::TestServer;
use serial_test::serial;
use toni::{controller, get, injectable, module, Body as ToniBody, HttpRequest};

#[controller("/static", pub struct StaticController {})]
impl StaticController {
    #[get("/hello")]
    fn hello(_req: HttpRequest) -> ToniBody {
        ToniBody::text("Hello from static method".to_string())
    }

    #[get("/world")]
    fn world(_req: HttpRequest) -> ToniBody {
        ToniBody::text("World from static method".to_string())
    }
}

#[module(controllers: [StaticController], providers: [])]
impl StaticModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn static_method_controller() {
    let server = TestServer::start(StaticModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/static/hello"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Hello from static method");

    let resp = server
        .client()
        .get(server.url("/static/world"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "World from static method");
}

#[injectable(pub struct MixedService {})]
impl MixedService {
    pub fn get_instance_message(&self) -> String {
        "From instance method".to_string()
    }
}

#[controller("/mixed", pub struct MixedController {
    #[inject]
    service: MixedService,
})]
impl MixedController {
    #[get("/instance")]
    fn instance_method(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(self.service.get_instance_message())
    }

    #[get("/static")]
    fn static_method(_req: HttpRequest) -> ToniBody {
        ToniBody::text("From static method".to_string())
    }
}

#[module(controllers: [MixedController], providers: [MixedService])]
impl MixedModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn mixed_static_and_instance_methods() {
    let server = TestServer::start(MixedModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/mixed/instance"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "From instance method");

    let resp = server
        .client()
        .get(server.url("/mixed/static"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "From static method");
}

#[controller("/request-static", scope = "request", pub struct RequestScopedStaticController {})]
impl RequestScopedStaticController {
    #[get("/test")]
    fn test(_req: HttpRequest) -> ToniBody {
        ToniBody::text("Static method in request-scoped controller".to_string())
    }
}

#[module(controllers: [RequestScopedStaticController], providers: [])]
impl RequestScopedStaticModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_scoped_static_methods() {
    let server = TestServer::start(RequestScopedStaticModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/request-static/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.unwrap(),
        "Static method in request-scoped controller"
    );
}

#[controller("/async-static", pub struct AsyncStaticController {})]
impl AsyncStaticController {
    #[get("/test")]
    async fn test(_req: HttpRequest) -> ToniBody {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        ToniBody::text("Async static method".to_string())
    }
}

#[module(controllers: [AsyncStaticController], providers: [])]
impl AsyncStaticModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn async_static_methods() {
    let server = TestServer::start(AsyncStaticModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/async-static/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "Async static method");
}
