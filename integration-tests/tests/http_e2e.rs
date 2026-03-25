mod common;

use common::TestServer;
use serde::{Deserialize, Serialize};
use serial_test::serial;
use std::sync::atomic::{AtomicU32, Ordering};
use toni::{
    controller, extractors::Json, get, injectable, module, post, Body as ToniBody, HttpAdapter,
    HttpRequest, Request,
};
use toni_config::{Config, ConfigModule, ConfigService};

#[derive(Config, Clone)]
struct AppConfig {
    #[env("APP_ENV")]
    #[default("test".to_string())]
    pub env: String,
}

#[serial]
#[tokio_localset_test::localset_test]
async fn async_controller_methods_with_http_server() {
    #[injectable(pub struct AsyncService {})]
    impl AsyncService {
        pub async fn process(&self) -> String {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            "processed".to_string()
        }
    }

    #[controller("/api", pub struct TestController {
        #[inject]
        service: AsyncService,
    })]
    impl TestController {
        #[get("/async")]
        async fn async_endpoint(&self) -> ToniBody {
            let result = self.service.process().await;
            ToniBody::text(result)
        }
    }

    #[module(
        providers: [AsyncService],
        controllers: [TestController],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/async"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "processed");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn config_service_injection_in_controllers() {
    #[controller("/api", pub struct TestController {
        #[inject]
        config: ConfigService<AppConfig>,
    })]
    impl TestController {
        #[get("/env")]
        fn get_env(&self) -> ToniBody {
            ToniBody::text(self.config.get_ref().env.clone())
        }
    }

    #[module(
        imports: [ConfigModule::<AppConfig>::new()],
        controllers: [TestController],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/env"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "test");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn singleton_controllers_share_state() {
    static INSTANCE_COUNTER: AtomicU32 = AtomicU32::new(0);

    #[controller("/api", pub struct SingletonController {
        instance_id: u32,
    })]
    impl SingletonController {
        pub fn new() -> Self {
            let id = INSTANCE_COUNTER.fetch_add(1, Ordering::SeqCst);
            Self { instance_id: id }
        }

        #[get("/id")]
        fn get_id(&self) -> ToniBody {
            ToniBody::text(format!("{}", self.instance_id))
        }
    }

    #[module(controllers: [SingletonController])]
    impl TestModule {}

    INSTANCE_COUNTER.store(0, Ordering::SeqCst);
    let server = TestServer::start(TestModule::module_definition()).await;

    for _ in 0..3 {
        let resp = server
            .client()
            .get(server.url("/api/id"))
            .send()
            .await
            .unwrap();
        let body = resp.text().await.unwrap();
        assert_eq!(body, "0");
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_scoped_controllers_create_per_request() {
    static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    #[controller("/api", scope = "request", pub struct RequestController {
        request_id: u32,
    })]
    impl RequestController {
        pub fn new() -> Self {
            let id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            Self { request_id: id }
        }

        #[get("/id")]
        fn get_id(&self) -> ToniBody {
            ToniBody::text(format!("{}", self.request_id))
        }
    }

    #[module(controllers: [RequestController])]
    impl TestModule {}

    REQUEST_COUNTER.store(0, Ordering::SeqCst);
    let server = TestServer::start(TestModule::module_definition()).await;

    let resp1 = server
        .client()
        .get(server.url("/api/id"))
        .send()
        .await
        .unwrap();
    let body1 = resp1.text().await.unwrap();

    let resp2 = server
        .client()
        .get(server.url("/api/id"))
        .send()
        .await
        .unwrap();
    let body2 = resp2.text().await.unwrap();

    assert_ne!(body1, body2);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn optional_request_extractor() {
    #[controller("/api", pub struct TestController {})]
    impl TestController {
        #[get("/headers")]
        fn get_headers(&self, req: Request) -> ToniBody {
            let has_header = req.header("X-Test-Header").is_some();
            ToniBody::text(format!("{}", has_header))
        }
    }

    #[module(controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/headers"))
        .header("X-Test-Header", "value")
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "true");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn json_body_and_request_extraction() {
    #[derive(Serialize, Deserialize)]
    struct CreateUser {
        name: String,
        email: String,
    }

    #[controller("/api", pub struct TestController {})]
    impl TestController {
        #[post("/users")]
        fn create_user(&self, Json(user): Json<CreateUser>, req: Request) -> ToniBody {
            let content_type = req.header("content-type").unwrap_or("unknown");
            ToniBody::text(format!("created {} ({})", user.name, content_type))
        }
    }

    #[module(controllers: [TestController])]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let user = CreateUser {
        name: "John".to_string(),
        email: "john@example.com".to_string(),
    };

    let resp = server
        .client()
        .post(server.url("/api/users"))
        .json(&user)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("created John"));
    assert!(body.contains("application/json"));
}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_extensions_pattern() {
    use toni::async_trait;
    use toni::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};
    use toni::traits_helpers::MiddlewareConsumer;

    #[derive(Clone)]
    struct UserId(String);

    struct AuthMiddleware;

    #[async_trait]
    impl Middleware for AuthMiddleware {
        async fn handle(&self, mut req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
            req.extensions_mut().insert(UserId("user123".to_string()));
            next.run(req).await
        }
    }

    #[controller("/api", pub struct TestController {})]
    impl TestController {
        #[get("/user")]
        fn get_user(&self, req: Request) -> ToniBody {
            let user_id = req.extensions().get::<UserId>();
            match user_id {
                Some(id) => ToniBody::text(id.0.clone()),
                None => ToniBody::text("no_user".to_string()),
            }
        }
    }

    #[module(controllers: [TestController])]
    impl TestModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer.apply(AuthMiddleware).for_routes(vec!["/api/*"]);
        }
    }

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/user"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "user123");
}
