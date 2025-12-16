use app::app_module::AppModule;
use toni::{adapter::AxumAdapter, http_adapter::HttpAdapter, toni_factory::ToniFactory};

mod app;

#[tokio::main]
async fn main() {
    let axum_adapter = AxumAdapter::new();

    let app = ToniFactory::create(AppModule::module_definition(), axum_adapter);
    app.listen(3000, "127.0.0.1").await;
}
