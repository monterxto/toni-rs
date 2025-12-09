mod common;

use common::TestServer;
use serial_test::serial;
use toni::injector::ModuleRef;
use toni::{controller, controller_struct, get, injectable, module, Body as ToniBody, HttpRequest};

#[serial]
#[tokio_localset_test::localset_test]
async fn global_modules_attribute_syntax() {
    #[injectable(pub struct GlobalService {})]
    impl GlobalService {
        pub fn message(&self) -> String {
            "global".to_string()
        }
    }

    #[module(
        global: true,
        providers: [GlobalService],
        exports: [GlobalService],
    )]
    impl GlobalModule {}

    #[injectable(pub struct LocalService {
        #[inject]
        global: GlobalService,
    })]
    impl LocalService {
        pub fn get_message(&self) -> String {
            self.global.message()
        }
    }

    #[controller_struct(pub struct TestController {
        #[inject]
        service: LocalService,
    })]
    #[controller("")]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.service.get_message())
        }
    }

    #[module(
        imports: [GlobalModule],
        providers: [LocalService],
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
    assert_eq!(body, "global");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn module_ref_runtime_provider_access() {
    #[injectable(pub struct RuntimeService {})]
    impl RuntimeService {
        pub fn value(&self) -> i32 {
            42
        }
    }

    #[controller_struct(pub struct TestController {
        #[inject]
        module_ref: ModuleRef,
    })]
    #[controller("")]
    impl TestController {
        #[get("/test")]
        async fn test(&self, _req: HttpRequest) -> ToniBody {
            let service = self.module_ref.get::<RuntimeService>().await;
            ToniBody::Text(format!("{}", service.unwrap().value()))
        }
    }

    #[module(
        providers: [RuntimeService],
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
    assert_eq!(body, "42");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn nested_module_imports() {
    #[injectable(pub struct DatabaseService {})]
    impl DatabaseService {
        pub fn query(&self) -> String {
            "data".to_string()
        }
    }

    #[module(
        providers: [DatabaseService],
        exports: [DatabaseService],
    )]
    impl DatabaseModule {}

    #[injectable(pub struct FeatureService {
        #[inject]
        db: DatabaseService,
    })]
    impl FeatureService {
        pub fn get_data(&self) -> String {
            self.db.query()
        }
    }

    #[module(
        imports: [DatabaseModule],
        providers: [FeatureService],
        exports: [FeatureService],
    )]
    impl FeatureModule {}

    #[controller_struct(pub struct TestController {
        #[inject]
        feature: FeatureService,
    })]
    #[controller("")]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.feature.get_data())
        }
    }

    #[module(
        imports: [FeatureModule],
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
async fn module_exports_selective_providers() {
    #[injectable(pub struct PublicService {})]
    impl PublicService {
        pub fn data(&self) -> String {
            "public".to_string()
        }
    }

    #[injectable(pub struct PrivateService {})]
    impl PrivateService {}

    #[module(
        providers: [PublicService, PrivateService],
        exports: [PublicService],
    )]
    impl SourceModule {}

    #[injectable(pub struct ConsumerService {
        #[inject]
        public: PublicService,
    })]
    impl ConsumerService {
        pub fn get_data(&self) -> String {
            self.public.data()
        }
    }

    #[controller_struct(pub struct TestController {
        #[inject]
        consumer: ConsumerService,
    })]
    #[controller("")]
    impl TestController {
        #[get("/test")]
        fn test(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(self.consumer.get_data())
        }
    }

    #[module(
        imports: [SourceModule],
        providers: [ConsumerService],
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
    assert_eq!(body, "public");
}
