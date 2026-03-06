#[path = "adapter/mod.rs"]
pub mod adapter;
mod application_context;
pub mod builtin_module;
pub mod di;
pub mod errors;
pub mod extractors;
#[path = "adapter/http_adapter.rs"]
pub mod http_adapter;
pub mod http_helpers;
pub mod injector;
pub mod middleware;
pub mod module_helpers;
pub mod provider_scope;
mod request;
mod router;
pub mod rpc;
mod scanner;
mod structs_helpers;
mod toni_application;
pub mod toni_factory;
pub mod traits_helpers;
pub mod websocket;

// Re-exports for adapter crates
pub use adapter::{WebSocketAdapter, WsConnectionCallbacks};
pub use http_adapter::HttpAdapter;
pub use http_helpers::{
    Body, HttpMethod, HttpRequest, HttpResponse, HttpResponseBuilder, RouteMetadata, ToResponse,
};
pub use injector::{InstanceWrapper, Protocol, ProtocolType};
pub use rpc::{RpcContext, RpcData};
pub use websocket::{
    DisconnectReason, GatewayTrait, GatewayWrapper, WsClient, WsError, WsHandshake, WsMessage,
};

// Re-export built-in providers
pub use request::{Request, RequestManager};

// Re-export ModuleRef for dynamic DI resolution
pub use injector::{Context, IntoToken, ModuleRef};

pub use application_context::ToniApplicationContext;

// Re-export dependencies used in macro-generated code
// This allows users to only depend on `toni` without needing to add these explicitly
pub use async_trait::async_trait;
pub use rustc_hash::FxHashMap;

// Re-export provider scope
pub use provider_scope::ProviderScope;

// Re-export trait so users wont have to import manually
pub use extractors::FromRequest;

// Re-export macros
pub use toni_macros::*;

// Re-export enhancer marker macros with better namespacing to avoid conflicts
pub mod enhancer {
    pub use toni_macros::{error_handler, guard, interceptor, middleware, pipe};
}

pub use toni_factory::ToniFactory;

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::task::JoinHandle;

    #[tokio::test]
    #[ignore = "Requires server to be running"]
    async fn test_server() {
        let server_handle: JoinHandle<()> = tokio::spawn(async {
            // let factory = ToniFactory::new();
            // let mut axum_adapter = AxumAdapter::new();
            // let mut app = ToniFactory::create(app_module, axum_adapter).unwrap();
            // app.listen(3000, "127.0.0.1").await;
            // let mut app = match app {
            //     Ok(app) => {
            //         app
            //     }
            //     Err(e) => panic!("sda")
            // };
            // let axum_adapter2 = AxumAdapter::new();
            // axum_adapter.add_route(&"/ta".to_string(), HttpMethod::GET, Box::new(GetUserNameController));
            // axum_adapter.listen(3000, "127.0.0.1").await;
            // app.listen(3000, "127.0.0.1");
            // servera.get("/ta", |req| Box::pin(route_adapter(req, &Handler)));
            // servera.post("/hello2", |req| Box::pin(route_adapter(req, &Handler2)));
            // servera.listen(3000, "127.0.0.1").await
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let client = reqwest::Client::new();
        let response = client.get("http://localhost:3000/names").send().await;
        let res = match response {
            Ok(res) => res,
            Err(e) => panic!("{}", e),
        };

        let body = match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(e) => panic!("{}", e),
        };

        assert_eq!(body["message"].as_str().unwrap(), "John Doe");
        server_handle.abort();
    }
}
