mod common;

use common::TestServer;
use serial_test::serial;
use toni::{controller, get, module, Body as ToniBody, HttpRequest, Request};

#[controller("/test", pub struct TestController {
    #[inject]
    request: Request,
})]
impl TestController {
    #[get("/info")]
    fn get_info(&self, _req: HttpRequest) -> ToniBody {
        let method = self.request.method();
        let uri = self.request.uri();
        ToniBody::Text(format!("Method: {}, URI: {}", method, uri))
    }

    #[get("/headers")]
    fn get_headers(&self, _req: HttpRequest) -> ToniBody {
        let content_type = self.request.header("content-type").unwrap_or("not found");
        ToniBody::Text(format!("Content-Type: {}", content_type))
    }
}

#[module(controllers: [TestController], providers: [])]
impl TestModule {}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_auto_injected_without_providers_entry() {
    let server = TestServer::start(TestModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/test/info"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("Method: GET"));
    assert!(body.contains("URI: /test/info"));

    let resp = server
        .client()
        .get(server.url("/test/headers"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.text().await.unwrap().contains("Content-Type: application/json"));
}
