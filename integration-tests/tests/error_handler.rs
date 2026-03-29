// Error handlers are the framework's last line of defense: a controller returning
// Err(HttpError) must reach a registered handler and map to the correct HTTP status.
//
// Two contracts tested here:
//   1. A global handler (registered on ToniFactory) intercepts HttpError and converts
//      it to the right status + body — verifying the default error propagation path.
//   2. A method-level handler runs before the global fallback (chain of responsibility):
//      the method handler owns 400s; everything else falls through to global.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use serial_test::serial;
use toni::{
    async_trait,
    controller, get,
    errors::HttpError,
    injector::Context,
    module,
    toni_factory::ToniFactory,
    traits_helpers::{ErrorHandler, ErrorResponse},
    Body as ToniBody, HttpRequest, HttpResponse,
};
use toni_axum::AxumAdapter;
use toni_macros::use_error_handlers;

static PORT: AtomicU16 = AtomicU16::new(36000);

struct GlobalHandler;

#[async_trait]
impl ErrorHandler for GlobalHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        _ctx: &Context,
    ) -> Option<ErrorResponse> {
        if let Some(e) = error.downcast_ref::<HttpError>() {
            let mut resp = HttpResponse::new();
            resp.status = e.status_code();
            resp.body = Some(ToniBody::text(format!("global:{}", e.message())));
            return Some(ErrorResponse::Http(resp));
        }
        None
    }
}

struct BadRequestHandler;

#[async_trait]
impl ErrorHandler for BadRequestHandler {
    async fn handle_error(
        &self,
        error: Box<dyn std::error::Error + Send>,
        _ctx: &Context,
    ) -> Option<ErrorResponse> {
        if let Some(e) = error.downcast_ref::<HttpError>() {
            if e.status_code() == 400 {
                let mut resp = HttpResponse::new();
                resp.status = 400;
                resp.body = Some(ToniBody::text(format!("method:{}", e.message())));
                return Some(ErrorResponse::Http(resp));
            }
        }
        None
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn global_error_handler_intercepts_http_error() {
    #[controller("/api", pub struct TestController {})]
    impl TestController {
        #[get("/missing")]
        fn missing(&self) -> Result<ToniBody, HttpError> {
            Err(HttpError::not_found("resource not found"))
        }
    }

    #[module(controllers: [TestController], providers: [])]
    impl TestModule {}

    let port = PORT.fetch_add(1, Ordering::SeqCst);

    tokio::task::spawn_local(async move {
        let mut factory = ToniFactory::new();
        factory.use_global_error_handler(Arc::new(GlobalHandler));
        let mut app = factory.create_with(TestModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new(), port, "127.0.0.1")
            .unwrap();
        let _ = app.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/api/missing", port))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.text().await.unwrap(), "global:resource not found");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn method_error_handler_runs_before_global() {
    #[controller("/api", pub struct TestController {})]
    impl TestController {
        #[get("/bad")]
        #[use_error_handlers(BadRequestHandler {})]
        fn bad(&self) -> Result<ToniBody, HttpError> {
            Err(HttpError::bad_request("invalid input"))
        }

        #[get("/gone")]
        #[use_error_handlers(BadRequestHandler {})]
        fn gone(&self) -> Result<ToniBody, HttpError> {
            Err(HttpError::not_found("not found"))
        }
    }

    #[module(controllers: [TestController], providers: [])]
    impl TestModule {}

    let port = PORT.fetch_add(1, Ordering::SeqCst);

    tokio::task::spawn_local(async move {
        let mut factory = ToniFactory::new();
        factory.use_global_error_handler(Arc::new(GlobalHandler));
        let mut app = factory.create_with(TestModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new(), port, "127.0.0.1")
            .unwrap();
        let _ = app.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::new();

    // 400: method handler claims it, global never runs
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/bad", port))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    assert_eq!(resp.text().await.unwrap(), "method:invalid input");

    // 404: method handler returns None (only handles 400), falls through to global
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/gone", port))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.text().await.unwrap(), "global:not found");
}
