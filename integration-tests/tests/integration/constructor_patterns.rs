use crate::common::TestServer;
use serial_test::serial;
use std::time::Duration;
use toni::{controller, get, injectable, module, provide, Body as ToniBody};

#[serial]
#[tokio_localset_test::localset_test]
async fn provider_constructor_patterns() {
    #[injectable(pub struct BaseService {
        value: String,
    })]
    impl BaseService {
        pub fn new() -> Self {
            Self {
                value: "base".to_string(),
            }
        }

        pub fn get_value(&self) -> String {
            self.value.clone()
        }
    }

    #[injectable(pub struct AutoNewService {
        base_value: String,
    })]
    impl AutoNewService {
        pub fn new(base: BaseService) -> Self {
            Self {
                base_value: base.get_value(),
            }
        }

        pub fn get_value(&self) -> String {
            self.base_value.clone()
        }
    }

    #[injectable(init = "create", pub struct CustomInitService {
        combined: String,
    })]
    impl CustomInitService {
        fn create(base: BaseService) -> Self {
            Self {
                combined: format!("custom:{}", base.get_value()),
            }
        }

        pub fn get_value(&self) -> String {
            self.combined.clone()
        }
    }

    #[injectable(
        init = "build",
        pub struct ComplexInitService {
            base_value: String,
            settings: Vec<String>,
            timeout: Duration,
        }
    )]
    impl ComplexInitService {
        fn build(base: BaseService) -> Self {
            let mut settings = Vec::new();
            settings.push(format!("setting:{}", base.get_value()));
            settings.push("s2".to_string());

            Self {
                base_value: base.get_value(),
                settings,
                timeout: Duration::from_secs(60),
            }
        }

        pub fn get_settings_count(&self) -> usize {
            self.settings.len()
        }
    }

    #[injectable]
    pub struct DefaultFallbackService {
        name: String,
        count: i32,
    }

    impl DefaultFallbackService {
        pub fn get_info(&self) -> String {
            format!("name='{}',count={}", self.name, self.count)
        }
    }

    #[controller("/providers", pub struct ProviderTestController {
        #[inject] base: BaseService,
        #[inject] auto: AutoNewService,
        #[inject] custom: CustomInitService,
        #[inject] complex: ComplexInitService,
        #[inject] fallback: DefaultFallbackService,
    })]
    impl ProviderTestController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(format!(
                "{}|{}|{}|{}|{}",
                self.base.get_value(),
                self.auto.get_value(),
                self.custom.get_value(),
                self.complex.get_settings_count(),
                self.fallback.get_info()
            ))
        }
    }

    #[module(
        providers: [
            BaseService,
            AutoNewService,
            CustomInitService,
            ComplexInitService,
            DefaultFallbackService,
        ],
        controllers: [ProviderTestController]
    )]
    impl ProviderPatternsModule {}

    let server = TestServer::start(ProviderPatternsModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/providers/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "base|base|custom:base|2|name='',count=0");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn controller_constructor_patterns() {
    #[injectable(pub struct DataService {
        value: String,
    })]
    impl DataService {
        pub fn new() -> Self {
            Self {
                value: "data".to_string(),
            }
        }

        pub fn get_value(&self) -> String {
            self.value.clone()
        }
    }

    #[controller("/auto", pub struct AutoNewController {
        data_value: String,
    })]
    impl AutoNewController {
        pub fn new(data: DataService) -> Self {
            Self {
                data_value: data.get_value(),
            }
        }

        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(format!("auto: {}", self.data_value))
        }
    }

    #[controller("/custom", init = "create", pub struct CustomInitController {
        combined: String,
    })]
    impl CustomInitController {
        fn create(data: DataService) -> Self {
            Self {
                combined: format!("custom: {}", data.get_value()),
            }
        }

        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.combined.clone())
        }
    }

    #[controller("/default", pub struct DefaultFallbackController {
        name: String,
        count: i32,
    })]
    impl DefaultFallbackController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(format!("name='{}', count={}", self.name, self.count))
        }
    }

    #[module(
        providers: [DataService],
        controllers: [AutoNewController, CustomInitController, DefaultFallbackController]
    )]
    impl ControllerPatternsModule {}

    let server = TestServer::start(ControllerPatternsModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/auto/test"))
        .send()
        .await
        .unwrap();
    assert!(resp.text().await.unwrap().contains("data"));

    let resp = server
        .client()
        .get(server.url("/custom/test"))
        .send()
        .await
        .unwrap();
    assert!(resp.text().await.unwrap().contains("custom"));

    let resp = server
        .client()
        .get(server.url("/default/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "name='', count=0");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn constructor_param_injection_patterns() {
    const DB_TOKEN: &str = "CustomDatabase";

    #[injectable(pub struct DatabaseService {})]
    impl DatabaseService {
        pub fn query(&self) -> String {
            "data".to_string()
        }
    }

    #[injectable(pub struct ConfigService {})]
    impl ConfigService {
        pub fn get_config(&self) -> String {
            "config".to_string()
        }
    }

    #[injectable(pub struct CacheService {})]
    impl CacheService {
        pub fn get_cache(&self) -> String {
            "cache".to_string()
        }
    }

    // #[inject] on params is redundant but supported for explicitness
    #[injectable(pub struct BasicParamService {})]
    impl BasicParamService {
        fn new(#[inject] _db: DatabaseService) -> Self {
            Self {}
        }

        pub fn get_data(&self, db: &DatabaseService) -> String {
            format!("basic:{}", db.query())
        }
    }

    #[injectable(pub struct TokenParamService {})]
    impl TokenParamService {
        fn new(#[inject(DB_TOKEN)] _db: DatabaseService) -> Self {
            Self {}
        }

        pub fn get_data(&self, db: &DatabaseService) -> String {
            format!("token:{}", db.query())
        }
    }

    #[injectable(pub struct MixedParamService {})]
    impl MixedParamService {
        fn new(
            #[inject] _config: ConfigService,
            _cache: CacheService, // No #[inject], still works via type token
        ) -> Self {
            Self {}
        }

        pub fn get_info(&self, config: &ConfigService, cache: &CacheService) -> String {
            format!("{}-{}", config.get_config(), cache.get_cache())
        }
    }

    #[controller("/basic", pub struct BasicParamController {
        #[inject]
        db: DatabaseService,
        #[inject]
        service: BasicParamService,
    })]
    impl BasicParamController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.service.get_data(&self.db))
        }
    }

    #[controller("/token", pub struct TokenParamController {
        #[inject(DB_TOKEN)]
        db: DatabaseService,
        #[inject]
        service: TokenParamService,
    })]
    impl TokenParamController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.service.get_data(&self.db))
        }
    }

    #[controller("/mixed", pub struct MixedParamController {
        #[inject]
        config: ConfigService,
        #[inject]
        cache: CacheService,
        #[inject]
        service: MixedParamService,
    })]
    impl MixedParamController {
        #[get("/test")]
        fn test(&self) -> ToniBody {
            ToniBody::text(self.service.get_info(&self.config, &self.cache))
        }
    }

    #[module(
        providers: [
            DatabaseService,
            provide!(DB_TOKEN, provider(DatabaseService)),
            ConfigService,
            CacheService,
            BasicParamService,
            TokenParamService,
            MixedParamService,
        ],
        controllers: [BasicParamController, TokenParamController, MixedParamController]
    )]
    impl ParamInjectionModule {}

    let server = TestServer::start(ParamInjectionModule::module_definition()).await;

    let resp = server
        .client()
        .get(server.url("/basic/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "basic:data");

    let resp = server
        .client()
        .get(server.url("/token/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "token:data");

    let resp = server
        .client()
        .get(server.url("/mixed/test"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "config-cache");
}
