use toni::{
    controller, get, http_helpers::Body as ToniBody, injectable, module, post, HttpAdapter,
    HttpRequest,
};
use toni_actix::ActixAdapter;

// Simple service for testing
#[injectable(
    pub struct TestService;
)]
impl TestService {
    pub fn get_greeting(&self) -> String {
        "Hello from Actix!".to_string()
    }

    pub fn echo(&self, message: String) -> String {
        format!("Echo: {}", message)
    }
}

// Simple controller for testing
#[controller(
    "/test",
    pub struct TestController {
        #[inject]
        test_service: TestService,
    }
)]
impl TestController {
    #[get("/hello")]
    fn hello(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::text(self.test_service.get_greeting())
    }

    #[post("/echo")]
    fn echo(&self, req: HttpRequest) -> ToniBody {
        let message = String::from_utf8_lossy(req.body()).into_owned();
        ToniBody::text(self.test_service.echo(message))
    }
}

// Test module
#[module(
    imports: [],
    controllers: [TestController],
    providers: [TestService],
    exports: []
)]
impl TestModule {}

#[actix_rt::test]
async fn test_actix_e2e() {
    use std::time::Duration;
    use toni::toni_factory::ToniFactory;

    let port = 18081;
    let local = tokio::task::LocalSet::new();

    // Spawn server in background
    local.spawn_local(async move {
        let mut app = ToniFactory::create(TestModule::module_definition()).await;
        app.use_http_adapter(ActixAdapter::new("127.0.0.1", port))
            .unwrap();
        let _ = app.start().await;
    });

    // Run tests within the LocalSet
    local
        .run_until(async move {
            // Give the server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;

            let client = reqwest::Client::new();

            // Test GET request
            let get_response = client
                .get(format!("http://127.0.0.1:{}/test/hello", port))
                .send()
                .await
                .expect("GET request failed");

            assert_eq!(get_response.status(), 200);
            assert_eq!(get_response.text().await.unwrap(), "Hello from Actix!");

            // Test POST request
            let post_response = client
                .post(format!("http://127.0.0.1:{}/test/echo", port))
                .body("test message")
                .send()
                .await
                .expect("POST request failed");

            assert_eq!(post_response.status(), 200);
            assert_eq!(post_response.text().await.unwrap(), "Echo: test message");
        })
        .await;
}
