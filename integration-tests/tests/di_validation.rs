#![allow(dead_code, unused_variables)]

use serial_test::serial;
use toni::{injectable, module, ToniFactory};

#[serial]
#[tokio_localset_test::localset_test]
async fn valid_singleton_injects_singleton() {
    #[injectable(pub struct ServiceA {})]
    impl ServiceA {}

    #[injectable(pub struct ServiceB {
        #[inject]
        dep: ServiceA
    })]
    impl ServiceB {}

    #[module(providers: [ServiceA, ServiceB])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
async fn valid_request_injects_singleton() {
    #[injectable(pub struct SingletonService {})]
    impl SingletonService {}

    #[injectable(scope = "request", pub struct RequestService {
        #[inject]
        dep: SingletonService
    })]
    impl RequestService {}

    #[module(providers: [SingletonService, RequestService])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
async fn valid_transient_injects_any_scope() {
    #[injectable(pub struct SingletonService {})]
    impl SingletonService {}

    #[injectable(scope = "request", pub struct RequestService {})]
    impl RequestService {}

    #[injectable(scope = "transient", pub struct TransientService {
        #[inject]
        singleton: SingletonService,
        #[inject]
        request: RequestService,
    })]
    impl TransientService {}

    #[module(providers: [SingletonService, RequestService, TransientService])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
#[should_panic(expected = "Scope validation error")]
async fn singleton_cannot_inject_request_scoped() {
    #[injectable(scope = "request", pub struct RequestService {})]
    impl RequestService {}

    #[injectable(pub struct SingletonService {
        #[inject]
        request_dep: RequestService
    })]
    impl SingletonService {}

    #[module(providers: [RequestService, SingletonService])]
    impl InvalidModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(InvalidModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
async fn singleton_can_inject_transient() {
    #[injectable(scope = "transient", pub struct TransientService {})]
    impl TransientService {}

    #[injectable(pub struct SingletonService {
        #[inject]
        transient_dep: TransientService
    })]
    impl SingletonService {}

    #[module(providers: [TransientService, SingletonService])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
async fn request_can_inject_transient() {
    #[injectable(scope = "transient", pub struct TransientService {})]
    impl TransientService {}

    #[injectable(scope = "request", pub struct RequestService {
        #[inject]
        transient_dep: TransientService
    })]
    impl RequestService {}

    #[module(providers: [TransientService, RequestService])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
async fn complex_valid_hierarchy() {
    #[injectable(pub struct BaseService {})]
    impl BaseService {}

    #[injectable(pub struct MiddleService {
        #[inject]
        base: BaseService
    })]
    impl MiddleService {}

    #[injectable(scope = "request", pub struct TopService {
        #[inject]
        middle: MiddleService,
        #[inject]
        base: BaseService
    })]
    impl TopService {}

    #[module(providers: [BaseService, MiddleService, TopService])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}

#[serial]
#[tokio_localset_test::localset_test]
#[should_panic(expected = "Scope validation error")]
async fn explicit_singleton_with_request_fails() {
    #[injectable(scope = "request", pub struct RequestService {})]
    impl RequestService {}

    #[injectable(scope = "singleton", pub struct ExplicitSingleton {
        #[inject]
        request_dep: RequestService
    })]
    impl ExplicitSingleton {}

    #[module(providers: [RequestService, ExplicitSingleton])]
    impl TestModule {}

    let factory = toni::toni_factory::ToniFactory::new();
    let _app = ToniFactory::create(TestModule::module_definition()).await;
}
