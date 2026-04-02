
use crate::common::TestServer;
use serial_test::serial;
use std::time::Duration;
use toni::{
    controller, get, injectable, module, provider_alias, provider_factory, provider_token,
    provider_value, Body as ToniBody,
};

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_value_injects_constant() {
    #[controller(pub struct TestController {})]
    impl TestController {
        #[get("/port")]
        fn get_port(&self) -> ToniBody {
            ToniBody::text("3000".to_string())
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
        fn test(&self) -> ToniBody {
            ToniBody::text("ok".to_string())
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
        fn test(&self) -> ToniBody {
            ToniBody::text("ok".to_string())
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
        fn test(&self) -> ToniBody {
            ToniBody::text("ok".to_string())
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
    #[injectable(pub struct ConfigService {
        env: String,
    })]
    impl ConfigService {
        pub fn new() -> Self {
            Self {
                env: "production".to_string(),
            }
        }
        pub fn get_env(&self) -> &str {
            &self.env
        }
    }

    // Injects ConfigService twice: once by type, once through the "Config" alias.
    // If the alias registration doesn't create a working resolution path,
    // DI startup panics and the test never reaches the HTTP assertion.
    #[injectable(pub struct VerifyService {
        #[inject]
        by_type: ConfigService,
        #[inject("Config")]
        by_alias: ConfigService,
    })]
    impl VerifyService {
        pub fn report(&self) -> String {
            format!("{}|{}", self.by_type.get_env(), self.by_alias.get_env())
        }
    }

    #[controller("", pub struct TestController {
        #[inject] verify: VerifyService,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.verify.report())
        }
    }

    #[module(
        providers: [
            ConfigService,
            provider_alias!("Config", ConfigService),
            VerifyService,
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
    assert_eq!(resp.text().await.unwrap(), "production|production");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_token_for_custom_types() {
    #[injectable(pub struct DatabaseService {
        host: String,
    })]
    impl DatabaseService {
        pub fn new() -> Self {
            Self {
                host: "localhost:5432".to_string(),
            }
        }
        pub fn get_host(&self) -> &str {
            &self.host
        }
    }

    // Injects DatabaseService by its "PRIMARY_DB" token.
    // If token registration doesn't wire the resolution path, startup panics.
    #[injectable(pub struct AppService {
        #[inject("PRIMARY_DB")]
        primary: DatabaseService,
    })]
    impl AppService {
        pub fn get_info(&self) -> String {
            self.primary.get_host().to_string()
        }
    }

    #[controller("", pub struct TestController {
        #[inject] app: AppService,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.app.get_info())
        }
    }

    #[module(
        providers: [
            DatabaseService,
            provider_token!("PRIMARY_DB", DatabaseService),
            AppService,
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
    assert_eq!(resp.text().await.unwrap(), "localhost:5432");
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

    // Consumes one alias and one token to prove they're injectable alongside
    // value/factory providers in the same module.
    #[injectable(pub struct AliasTokenConsumer {
        #[inject("Config")]
        config_via_alias: ConfigService,
        #[inject("PRIMARY_CONFIG")]
        config_via_token: ConfigService,
    })]
    impl AliasTokenConsumer {
        pub fn report(&self) -> String {
            format!(
                "{}|{}",
                self.config_via_alias.get_env(),
                self.config_via_token.get_env()
            )
        }
    }

    #[controller("", pub struct TestController {
        #[inject] consumer: AliasTokenConsumer,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.consumer.report())
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
            AliasTokenConsumer,
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
    assert_eq!(resp.text().await.unwrap(), "production|production");
}
