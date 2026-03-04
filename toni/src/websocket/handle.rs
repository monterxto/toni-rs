use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::{broadcast::ClientId, Sender, WsMessage};

pub struct WsGatewayHandle {
    clients: Arc<RwLock<HashMap<ClientId, Arc<dyn Sender>>>>,
}

impl WsGatewayHandle {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Send to one specific client, waiting for buffer space.
    pub async fn emit(&self, client_id: &str, msg: WsMessage) {
        let sender = self.clients.read().get(client_id).cloned();
        if let Some(sender) = sender {
            let _ = sender.send(msg).await;
        }
    }

    /// Fire-and-forget broadcast to all connected clients.
    pub async fn broadcast(&self, msg: WsMessage) {
        let senders: Vec<Arc<dyn Sender>> = self.clients.read().values().cloned().collect();
        for sender in senders {
            let _ = sender.try_send(msg.clone());
        }
    }

    /// Fire-and-forget broadcast to all connected clients except one.
    pub async fn broadcast_except(&self, exclude: &str, msg: WsMessage) {
        let senders: Vec<Arc<dyn Sender>> = self
            .clients
            .read()
            .iter()
            .filter(|(id, _)| id.as_str() != exclude)
            .map(|(_, s)| s.clone())
            .collect();
        for sender in senders {
            let _ = sender.try_send(msg.clone());
        }
    }

    pub fn connected_clients(&self) -> Vec<ClientId> {
        self.clients.read().keys().cloned().collect()
    }

    pub(crate) fn register(&self, id: ClientId, sender: Arc<dyn Sender>) {
        self.clients.write().insert(id, sender);
    }

    pub(crate) fn unregister(&self, id: &str) {
        self.clients.write().remove(id);
    }
}

impl Default for WsGatewayHandle {
    fn default() -> Self {
        Self::new()
    }
}
