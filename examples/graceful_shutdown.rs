//! Graceful shutdown example with signal handling
//!
//! Demonstrates:
//! - Lifecycle hooks (OnModuleDestroy, BeforeApplicationShutdown, OnApplicationShutdown)
//! - Graceful WebSocket connection cleanup
//! - Multi-signal handling (SIGTERM, SIGINT/Ctrl+C)
//! - Cross-platform signal support (Unix/Windows)
//! - Application.close() for cleanup
//!
//! Run with: cargo run --example graceful_shutdown
//! Connect with: wscat -c ws://localhost:3000/ws
//! Press Ctrl+C or send SIGTERM to trigger graceful shutdown

use toni::websocket::{BroadcastModule, BroadcastService, WsClient, WsError, WsMessage};
use toni::*;
use toni_macros::{module, websocket_gateway};

#[websocket_gateway("/ws", pub struct SimpleGateway {
    broadcast: BroadcastService,
})]
impl SimpleGateway {
    pub fn new(broadcast: BroadcastService) -> Self {
        Self { broadcast }
    }

    #[on_connect]
    async fn handle_connect(&self, client: &WsClient) -> Result<(), WsError> {
        println!("✅ Client {} connected", client.id);
        let welcome = WsMessage::text(&format!("Welcome! Your ID: {}", client.id));
        self.broadcast
            .to_client(&client.id)
            .send(welcome)
            .await
            .ok();
        Ok(())
    }

    #[on_disconnect]
    async fn handle_disconnect(&self, client: &WsClient) {
        println!("❌ Client {} disconnected", client.id);
    }

    #[subscribe_message("ping")]
    async fn handle_ping(
        &self,
        client: WsClient,
        _message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        println!("🏓 Ping from {}", client.id);
        Ok(Some(WsMessage::text("pong")))
    }
}

#[module(providers: [SimpleGateway], imports:[BroadcastModule::new()])]
struct AppModule;

/// Returns the signal name so it can be forwarded to lifecycle hooks.
async fn shutdown_signal() -> String {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("failed to listen for SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to listen for SIGINT");

        tokio::select! {
            _ = sigterm.recv() => "SIGTERM".to_string(),
            _ = sigint.recv() => "SIGINT".to_string(),
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
        "SIGINT".to_string()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Starting server with graceful shutdown support...\n");

    let mut app = ToniFactory::new()
        .create_with(AppModule, toni_axum::AxumAdapter::new())
        .await;

    println!("📡 WebSocket server running on ws://localhost:3000/ws");
    println!("📝 Try: wscat -c ws://localhost:3000/ws");
    println!("⚡ Press Ctrl+C or send SIGTERM to trigger graceful shutdown\n");

    tokio::select! {
        _ = app.listen(3000, "127.0.0.1") => {
            println!("Server stopped normally");
        }
        signal = shutdown_signal() => {
            println!("\n\n⚡ Received {} signal", signal);
            println!("🛑 Initiating graceful shutdown...");

            // Triggers hooks in order: onModuleDestroy → beforeApplicationShutdown → onApplicationShutdown
            app.close().await?;

            println!("✅ Shutdown complete!");
        }
    }

    Ok(())
}
