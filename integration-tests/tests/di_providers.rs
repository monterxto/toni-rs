mod common;

use common::TestServer;
use serial_test::serial;
use std::time::Duration;
use toni::{
    controller, get, injectable, module, provider_alias, provider_factory, provider_token,
    provider_value, Body as ToniBody, HttpRequest,
};

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_value_injects_constant() {
    #[controller(pub struct TestController {})]
    impl TestController {
        #[get("/port")]
        fn get_port(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("3000".to_string())
        }
    }

    #[module(
        providers: [provider_value!("PORT", 3000_u16)],
        controllers: [TestController]
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/port"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_factory_sync_without_deps() {
    use std::sync::atomic::{AtomicU32, Ordering};

    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

    #[controller("", pub struct TestController {})]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [provider_factory!("REQUEST_ID", || {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
            format!("req_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis())
        })],
        controllers: [TestController]
    )]
    impl TestModule {}

    CALL_COUNT.store(0, Ordering::SeqCst);
    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_factory_sync_with_deps() {
    #[injectable(pub struct ConfigService {
        env: String,
    })]
    impl ConfigService {
        pub fn new() -> Self {
            Self {
                env: "production".to_string(),
            }
        }

        pub fn get_env(&self) -> String {
            self.env.clone()
        }
    }

    #[controller("", pub struct TestController {})]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [
            ConfigService,
            provider_factory!("APP_INFO", |config: ConfigService| {
                format!("App running in {} mode", config.get_env())
            })
        ],
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
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_factory_async_with_deps() {
    #[injectable(pub struct LoggerService {
        level: String,
    })]
    impl LoggerService {
        pub fn new() -> Self {
            Self {
                level: "info".to_string(),
            }
        }

        pub fn log(&self, msg: &str) -> String {
            format!("[{}] {}", self.level, msg)
        }
    }

    #[controller("", pub struct TestController {})]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [
            LoggerService,
            provider_factory!("ASYNC_STATUS", async |logger: LoggerService| {
                tokio::time::sleep(Duration::from_millis(1)).await;
                logger.log("System initialized")
            })
        ],
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
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_alias_creates_alternate_token() {
    #[injectable(pub struct ConfigService {})]
    impl ConfigService {
        pub fn new() -> Self {
            Self {}
        }
    }

    #[controller("", pub struct TestController {})]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [
            ConfigService,
            provider_alias!("Config", ConfigService)
        ],
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
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_token_for_custom_types() {
    #[injectable(pub struct DatabaseService {})]
    impl DatabaseService {
        pub fn new() -> Self {
            Self {}
        }
    }

    #[controller("", pub struct TestController {})]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [
            DatabaseService,
            provider_token!("PRIMARY_DB", DatabaseService)
        ],
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
    assert_eq!(resp.status(), 200);
}

#[serial]
#[tokio_localset_test::localset_test]
async fn all_provider_variants_work_together() {
    #[injectable(pub struct ConfigService {
        env: String,
    })]
    impl ConfigService {
        pub fn new() -> Self {
            Self {
                env: "production".to_string(),
            }
        }

        pub fn get_env(&self) -> String {
            self.env.clone()
        }
    }

    #[injectable(pub struct LoggerService {
        level: String,
    })]
    impl LoggerService {
        pub fn new() -> Self {
            Self {
                level: "info".to_string(),
            }
        }

        pub fn log(&self, msg: &str) -> String {
            format!("[{}] {}", self.level, msg)
        }
    }

    #[controller("", pub struct TestController {})]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text("ok".to_string())
        }
    }

    #[module(
        providers: [
            ConfigService,
            LoggerService,
            provider_value!("APP_NAME", "ToniApp".to_string()),
            provider_value!("PORT", 3000_u16),
            provider_value!("TIMEOUT", Duration::from_secs(30)),
            provider_factory!("REQUEST_ID", || {
                format!("req_{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis())
            }),
            provider_factory!("APP_INFO", |config: ConfigService| {
                format!("App running in {} mode", config.get_env())
            }),
            provider_factory!("ASYNC_STATUS", async |logger: LoggerService| {
                tokio::time::sleep(Duration::from_millis(1)).await;
                logger.log("System initialized")
            }),
            provider_alias!("Config", ConfigService),
            provider_alias!("Logger", LoggerService),
            provider_alias!("APP_PORT", "PORT"),
            provider_token!("PRIMARY_CONFIG", ConfigService),
            provider_token!("SECONDARY_LOGGER", LoggerService),
        ],
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
    assert_eq!(resp.status(), 200);
}
