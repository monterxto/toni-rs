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
pub mod toni_application;
pub mod toni_factory;
pub mod traits_helpers;
pub mod websocket;

// Re-exports for adapter crates
pub use adapter::{
    RpcAdapter, RpcClientTransport, RpcMessageCallbacks, WebSocketAdapter, WsConnectionCallbacks,
};
pub use http_adapter::{HttpAdapter, HttpRequestCallbacks};
pub use http_helpers::{
    Body, BoxBody, HttpMethod, HttpRequest, HttpResponse, HttpResponseBuilder, IntoResponse,
    RequestBody, RequestBoxBody, RequestPart, RouteMetadata,
};
pub use injector::{InstanceWrapper, Protocol, ProtocolType};
pub use rpc::{RpcClient, RpcClientError, RpcContext, RpcControllerTrait, RpcData, RpcError};
pub use websocket::{
    BroadcastError, BroadcastModule, BroadcastService, BroadcastTarget, ClientId, DisconnectReason,
    GatewayTrait, GatewayWrapper, RoomId, SendError, TrySendError, WsClient, WsError,
    WsHandlerResult, WsHandshake, WsMessage, WsSink,
};

// Re-export built-in providers
pub use request::{Request, RequestFactory};

// Re-export ModuleRef for dynamic DI resolution
pub use injector::{Context, IntoToken, ModuleRef};

pub use application_context::ToniApplicationContext;

// Re-export dependencies used in macro-generated code
// This allows users to only depend on `toni` without needing to add these explicitly
pub use async_trait::async_trait;
pub use rustc_hash::FxHashMap;

// Re-export provider scope
pub use provider_scope::ProviderScope;

pub use traits_helpers::ProviderContext;

pub use errors::HttpError;

// Re-export trait so users wont have to import manually
pub use extractors::{BodyStream, FromRequest, FromRequestParts};

// Re-export macros
pub use toni_macros::*;

// Re-export enhancer marker macros with better namespacing to avoid conflicts
pub mod enhancer {
    pub use toni_macros::{error_handler, guard, interceptor, middleware, pipe};
}

pub use toni_application::ToniApplication;
pub use toni_factory::ToniFactory;

#[cfg(feature = "tower-compat")]
pub mod tower_compat;
#[cfg(feature = "tower-compat")]
pub use tower_compat::TowerLayer;
