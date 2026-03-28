mod common;

use common::TestServer;
use serial_test::serial;
use std::time::Duration;
use toni::{controller, get, injectable, module, provide, Body as ToniBody, HttpRequest};

#[injectable(pub struct ConfigService {
    env: String,
})]
impl ConfigService {
    pub fn new() -> Self {
        Self {
            env: "prod".to_string(),
        }
    }
}

#[injectable(pub struct DatabaseService {
    host: String,
})]
impl DatabaseService {
    pub fn new() -> Self {
        Self {
            host: "localhost:5432".to_string(),
        }
    }
}

#[injectable(pub struct CacheService {
    url: String,
})]
impl CacheService {
    pub fn new() -> Self {
        Self {
            url: "redis://localhost".to_string(),
        }
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn provide_macro_patterns() {
    #[injectable(pub struct AppService {
        #[inject("API_KEY")]
        api_key: String,

        #[inject("PORT")]
        port: u16,

        #[inject("TIMEOUT")]
        timeout: Duration,

        #[inject("MAX_CONNECTIONS")]
        max_connections: i32,

        #[inject("LOGGER")]
        logger: String,

        #[inject("PRIMARY_DB")]
        database: DatabaseService,

        #[inject("CACHE_ALIAS")]
        cache: CacheService,

        #[inject("EXPLICIT_VALUE")]
        explicit_value: String,

        #[inject("EXPLICIT_FACTORY")]
        explicit_factory: String,
    })]
    impl AppService {
        pub fn get_info(&self) -> String {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}",
                self.api_key,
                self.port,
                self.timeout.as_secs(),
                self.max_connections,
                self.logger,
                self.database.host,
                self.cache.url,
                self.explicit_value,
                self.explicit_factory
            )
        }
    }

    #[controller("/app", pub struct AppController {
        #[inject]
        app: AppService,
    })]
    impl AppController {
        #[get("/info")]
        fn info(&self) -> ToniBody {
            ToniBody::text(self.app.get_info())
        }
    }

    #[module(
        providers: [
            ConfigService,
            DatabaseService,
            CacheService,

            // Literals auto-detected as value providers
            provide!("API_KEY", "secret_key".to_string()),
            provide!("PORT", 8080_u16),
            provide!("TIMEOUT", Duration::from_secs(30)),

            // Closures auto-detected as factory providers
            provide!("MAX_CONNECTIONS", || 100_i32),
            provide!("LOGGER", |config: ConfigService| {
                format!("logger:{}", config.env)
            }),

            // provider() required - registers type under custom token
            provide!("PRIMARY_DB", provider(DatabaseService)),

            // existing() required - creates alias to existing provider
            provide!("CACHE_ALIAS", existing(CacheService)),

            // value() optional - explicit marker for clarity
            provide!("EXPLICIT_VALUE", value("explicit".to_string())),

            // factory() optional - explicit marker for clarity
            provide!("EXPLICIT_FACTORY", factory(|| "factory_result".to_string())),

            AppService,
        ],
        controllers: [AppController]
    )]
    impl UnifiedProvideModule {}

    let server = TestServer::start(UnifiedProvideModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/app/info"))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.text().await.unwrap(),
        "secret_key|8080|30|100|logger:prod|localhost:5432|redis://localhost|explicit|factory_result"
    );
}
