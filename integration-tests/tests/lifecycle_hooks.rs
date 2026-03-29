// Startup lifecycle ordering is the contract this file exists to prove.
//
// The framework guarantees:
//   module:on_module_init → provider:on_module_init
//     → module:on_application_bootstrap → provider:on_application_bootstrap
//
// on_module_init fires during ToniFactory::create(); on_application_bootstrap
// fires during app.start(). This split matters: providers that open connections
// in init are ready by the time bootstrap runs.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serial_test::serial;
use toni::{injectable, module, toni_factory::ToniFactory};
use toni_axum::AxumAdapter;

static PORT: AtomicU16 = AtomicU16::new(35000);

static EVENT_LOG: OnceLock<Arc<Mutex<Vec<&'static str>>>> = OnceLock::new();

fn get_log() -> Arc<Mutex<Vec<&'static str>>> {
    EVENT_LOG
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone()
}

#[injectable(pub struct HookedService {})]
impl HookedService {
    #[on_module_init]
    async fn on_init(&self) {
        get_log().lock().unwrap().push("provider:init");
    }

    #[on_application_bootstrap]
    async fn on_bootstrap(&self) {
        get_log().lock().unwrap().push("provider:bootstrap");
    }
}

#[module(providers: [HookedService])]
impl HookModule {
    #[on_module_init]
    fn on_module_init(&self) {
        get_log().lock().unwrap().push("module:init");
    }

    #[on_application_bootstrap]
    fn on_module_bootstrap(&self) {
        get_log().lock().unwrap().push("module:bootstrap");
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn startup_hooks_fire_in_order() {
    get_log().lock().unwrap().clear();

    let port = PORT.fetch_add(1, Ordering::SeqCst);

    tokio::task::spawn_local(async move {
        let mut app = ToniFactory::create(HookModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new(), port, "127.0.0.1")
            .unwrap();
        let _ = app.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let log = get_log().lock().unwrap().clone();
    assert_eq!(
        log,
        vec![
            "module:init",
            "provider:init",
            "module:bootstrap",
            "provider:bootstrap",
        ],
        "expected module init → provider init → module bootstrap → provider bootstrap"
    );
}
