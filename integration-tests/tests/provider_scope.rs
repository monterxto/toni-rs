//! Provider scope behavior tests
//!
//! When multiple fields inject the same provider token, deduplication behavior depends on scope:
//! - Singleton: Same instance app-wide, persists across requests
//! - Request: Same instance per HTTP request, fresh instance for each request
//! - Transient: Fresh instance per injection point at construction time

use serial_test::serial;
use toni::{controller, get, module, provider_factory, Body as ToniBody, HttpRequest};
use uuid::Uuid;

mod common;
use common::TestServer;

#[serial]
#[tokio_localset_test::localset_test]
async fn scope_behavior() {
    #[derive(Clone)]
    struct Counter {
        id: String,
    }

    impl Counter {
        fn new_singleton() -> Self {
            Self {
                id: Uuid::new_v4().to_string(),
            }
        }

        fn new_request() -> Self {
            Self {
                id: Uuid::new_v4().to_string(),
            }
        }

        fn new_transient() -> Self {
            Self {
                id: Uuid::new_v4().to_string(),
            }
        }
    }

    #[controller("/request-scoped", scope = "request", pub struct RequestController {
        #[inject("SINGLETON")]
        singleton1: Counter,
        #[inject("SINGLETON")]
        singleton2: Counter,
        #[inject("REQUEST")]
        request1: Counter,
        #[inject("REQUEST")]
        request2: Counter,
        #[inject("TRANSIENT")]
        transient1: Counter,
        #[inject("TRANSIENT")]
        transient2: Counter,
    })]
    impl RequestController {
        #[get("/get")]
        fn get_value(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(format!(
                "s:{}|{};r:{}|{};t:{}|{}",
                self.singleton1.id,
                self.singleton2.id,
                self.request1.id,
                self.request2.id,
                self.transient1.id,
                self.transient2.id
            ))
        }
    }

    #[controller("/singleton-scoped", pub struct SingletonController {
        #[inject("TRANSIENT")]
        transient1: Counter,
        #[inject("TRANSIENT")]
        transient2: Counter,
    })]
    impl SingletonController {
        #[get("/get")]
        fn get_value(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::Text(format!("t:{}|{}", self.transient1.id, self.transient2.id))
        }
    }

    #[module(
        controllers: [RequestController, SingletonController],
        providers: [
            provider_factory!("SINGLETON", || Counter::new_singleton(), Counter, scope = "singleton"),
            provider_factory!("REQUEST", || Counter::new_request(), scope = "request"),
            provider_factory!("TRANSIENT", || Counter::new_transient(), scope = "transient"),
        ],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;

    let resp1 = server
        .client()
        .get(server.url("/singleton-scoped/get"))
        .send()
        .await
        .unwrap();
    let singleton_body1 = resp1.text().await.unwrap();

    let singleton_ids1: Vec<&str> = singleton_body1
        .trim_start_matches("t:")
        .split('|')
        .collect();
    assert_ne!(
        singleton_ids1[0], singleton_ids1[1],
        "Transient should create different instances per injection point: {}",
        singleton_body1
    );

    let resp2 = server
        .client()
        .get(server.url("/singleton-scoped/get"))
        .send()
        .await
        .unwrap();
    let singleton_body2 = resp2.text().await.unwrap();

    assert_eq!(
        singleton_body1, singleton_body2,
        "Singleton controller should reuse same transient instances across requests: req1={}, req2={}",
        singleton_body1, singleton_body2
    );

    let req_resp1 = server
        .client()
        .get(server.url("/request-scoped/get"))
        .send()
        .await
        .unwrap();
    let req_body1 = req_resp1.text().await.unwrap();

    let parts1: Vec<&str> = req_body1.split(';').collect();
    let singleton_part1 = parts1[0].trim_start_matches("s:");
    let request_part1 = parts1[1].trim_start_matches("r:");
    let transient_part1 = parts1[2].trim_start_matches("t:");

    let s_ids1: Vec<&str> = singleton_part1.split('|').collect();
    assert_eq!(
        s_ids1[0], s_ids1[1],
        "Singleton should share same instance across injection points: {}",
        req_body1
    );

    let r_ids1: Vec<&str> = request_part1.split('|').collect();
    assert_eq!(
        r_ids1[0], r_ids1[1],
        "Request scope should share same instance within request: {}",
        req_body1
    );

    let t_ids1: Vec<&str> = transient_part1.split('|').collect();
    assert_ne!(
        t_ids1[0], t_ids1[1],
        "Transient should create different instances per injection point: {}",
        req_body1
    );

    let req_resp2 = server
        .client()
        .get(server.url("/request-scoped/get"))
        .send()
        .await
        .unwrap();
    let req_body2 = req_resp2.text().await.unwrap();

    let parts2: Vec<&str> = req_body2.split(';').collect();
    let singleton_part2 = parts2[0].trim_start_matches("s:");
    let request_part2 = parts2[1].trim_start_matches("r:");
    let transient_part2 = parts2[2].trim_start_matches("t:");

    let s_ids2: Vec<&str> = singleton_part2.split('|').collect();
    assert_eq!(
        s_ids1[0], s_ids2[0],
        "Singleton should persist same instance across requests: req1={}, req2={}",
        singleton_part1, singleton_part2
    );

    let r_ids2: Vec<&str> = request_part2.split('|').collect();
    assert_ne!(
        r_ids1[0], r_ids2[0],
        "Request scope should create new instance for new request: req1={}, req2={}",
        request_part1, request_part2
    );

    let t_ids2: Vec<&str> = transient_part2.split('|').collect();
    assert_ne!(
        t_ids1[0], t_ids2[0],
        "Transient should create new instances for new request: req1={}, req2={}",
        transient_part1, transient_part2
    );
}
