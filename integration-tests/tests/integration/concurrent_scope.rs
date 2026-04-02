// provider_scope.rs proves that sequential requests get isolated request-scoped
// instances. This file proves the same holds under concurrency: if the request
// context machinery has any shared mutable state (a stray RefCell, a map keyed
// on thread-id instead of request-id), concurrent requests would see each
// other's instances and this test would catch it.


use crate::common::TestServer;
use futures_util::future::join_all;
use serial_test::serial;
use toni::{controller, get, module, provider_factory, Body as ToniBody};
use uuid::Uuid;

#[serial]
#[tokio_localset_test::localset_test]
async fn request_scoped_instances_are_isolated_under_concurrency() {
    #[derive(Clone)]
    struct RequestId {
        id: String,
    }

    #[controller("/", scope = "request", pub struct TestController {
        #[inject("REQ_ID")]
        req_id: RequestId,
    })]
    impl TestController {
        #[get("/id")]
        fn get_id(&self) -> ToniBody {
            ToniBody::text(self.req_id.id.clone())
        }
    }

    #[module(
        controllers: [TestController],
        providers: [
            provider_factory!("REQ_ID", || RequestId { id: Uuid::new_v4().to_string() }, RequestId, scope = "request"),
        ],
    )]
    impl TestModule {}

    let server = TestServer::start(TestModule::module_definition()).await;

    const N: usize = 20;
    let futs: Vec<_> = (0..N)
        .map(|_| {
            let client = server.client().clone();
            let url = server.url("/id");
            async move { client.get(url).send().await.unwrap().text().await.unwrap() }
        })
        .collect();

    let ids = join_all(futs).await;

    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        N,
        "each of the {} concurrent requests must get a distinct request-scoped ID; \
        duplicates indicate scope context leak: {:?}",
        N,
        ids
    );
}
