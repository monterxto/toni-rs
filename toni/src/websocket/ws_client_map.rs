use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::{ClientId, WsMessage, WsSink};

/// Registry of active connections for one gateway path.
///
/// Holds the write channel for each connected client so the framework can
/// send a response back after `GatewayWrapper::handle_message` returns one.
/// This is the only place `Arc<dyn WsSink>` values live — `ConnectionManager`
/// delegates all sends here rather than storing its own copy.
pub(crate) struct WsClientMap {
    clients: Arc<RwLock<HashMap<ClientId, Arc<dyn WsSink>>>>,
}

impl WsClientMap {
    pub(crate) fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) fn register(&self, id: ClientId, sink: Arc<dyn WsSink>) {
        self.clients.write().insert(id, sink);
    }

    pub(crate) fn unregister(&self, id: &str) {
        self.clients.write().remove(id);
    }

    pub(crate) async fn send_to(&self, client_id: &str, msg: WsMessage) {
        let sink = self.clients.read().get(client_id).cloned();
        if let Some(sink) = sink {
            let _ = sink.send(msg).await;
        }
    }

    pub(crate) fn get_sink(&self, client_id: &str) -> Option<Arc<dyn WsSink>> {
        self.clients.read().get(client_id).cloned()
    }

    pub(crate) fn all_sinks(&self) -> Vec<Arc<dyn WsSink>> {
        self.clients.read().values().cloned().collect()
    }
}

impl Default for WsClientMap {
    fn default() -> Self {
        Self::new()
    }
}
