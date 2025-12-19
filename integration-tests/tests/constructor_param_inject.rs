mod common;

use common::TestServer;
use serial_test::serial;
use toni::{controller, get, injectable, module, provide, Body as ToniBody, HttpRequest};

#[serial]
#[tokio_localset_test::localset_test]
async fn constructor_param_inject_basic() {
    #[injectable(pub struct DatabaseService {})]
    impl DatabaseService {
        pub fn query(&self) -> String {
            "data".to_string()
        }
    }

    // Use #[inject] on constructor parameter (redundant but should work)
    #[injectable(pub struct ApiService {})]
    impl ApiService {
        fn new(#[inject] _db: DatabaseService) -> Self {
            Self {}
        }

        pub fn get_data(&self, db: &DatabaseService) -> String {
            db.query()
        }
    }

    #[controller("", pub struct TestController {
        #[inject]
        db: DatabaseService,
        #[inject]
        api: ApiService,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            let data = self.api.get_data(&self.db);
            ToniBody::Text(data)
        }
    }

    #[module(
        providers: [DatabaseService, ApiService],
        controllers: [TestController],
    )]
    impl AppModule {}

    let server = TestServer::start(AppModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "data");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn constructor_param_inject_custom_token() {
    //const DB_TOKEN: &str = "CustomDatabase";

    #[injectable(pub struct DatabaseService {})]
    impl DatabaseService {
        pub fn query(&self) -> String {
            "custom".to_string()
        }
    }

    // Use #[inject(token)] on constructor parameter to override DI token
    #[injectable(pub struct ApiService {})]
    impl ApiService {
        /*fn new(#[inject(DB_TOKEN)] _db: DatabaseService) -> Self {
            Self {}
        }*/
        fn new(#[inject("CustomDatabase")] _db: DatabaseService) -> Self {
            Self {}
        }

        pub fn get_data(&self, db: &DatabaseService) -> String {
            db.query()
        }
    }

    #[controller("", pub struct TestController {
        //#[inject(DB_TOKEN)]
        #[inject("CustomDatabase")]
        db: DatabaseService,
        #[inject]
        api: ApiService,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            let data = self.api.get_data(&self.db);
            ToniBody::Text(data)
        }
    }

    #[module(
        providers: [
            //provide!(DB_TOKEN, provider(DatabaseService)),
            provide!("CustomDatabase", provider(DatabaseService)),
            ApiService,
        ],
        controllers: [TestController],
    )]
    impl AppModule {}

    let server = TestServer::start(AppModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "custom");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn constructor_param_inject_mixed() {
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

    // Mix of #[inject] and no annotation on constructor params
    #[injectable(pub struct ApiService {})]
    impl ApiService {
        fn new(
            #[inject] _config: ConfigService,
            _cache: CacheService, // No #[inject], should still work (uses type token)
        ) -> Self {
            Self {}
        }

        pub fn get_info(&self, config: &ConfigService, cache: &CacheService) -> String {
            format!("{}-{}", config.get_config(), cache.get_cache())
        }
    }

    #[controller("", pub struct TestController {
        #[inject]
        config: ConfigService,
        #[inject]
        cache: CacheService,
        #[inject]
        api: ApiService,
    })]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            let info = self.api.get_info(&self.config, &self.cache);
            ToniBody::Text(info)
        }
    }

    #[module(
        providers: [ConfigService, CacheService, ApiService],
        controllers: [TestController],
    )]
    impl AppModule {}

    let server = TestServer::start(AppModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    assert_eq!(body, "config-cache");
}
