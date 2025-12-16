use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use toni::module_helpers::module_enum::ModuleDefinition;
use toni::toni_factory::ToniFactory;
use toni::HttpAdapter;
use toni_axum::AxumAdapter;

static PORT_COUNTER: AtomicU16 = AtomicU16::new(30000);

pub struct TestServer {
    pub port: u16,
    pub base_url: String,
    client: reqwest::Client,
}

impl TestServer {
    pub async fn start(module: ModuleDefinition) -> Self {
        let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let base_url = format!("http://127.0.0.1:{}", port);

        let local = tokio::task::LocalSet::new();

        local.spawn_local(async move {
            let adapter = AxumAdapter::new();

            let app = ToniFactory::create(module, adapter).await;
            let _ = app.listen(port, "127.0.0.1").await;
        });

        tokio::task::spawn_local(async move {
            local.await;
        });

        let client = reqwest::Client::new();
        tokio::time::sleep(Duration::from_millis(500)).await;

        Self {
            port,
            base_url,
            client,
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
