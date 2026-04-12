use std::{any::Any, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use sea_orm::{Database, DatabaseConnection};
use toni::{
    FxHashMap,
    traits_helpers::{Provider, ProviderContext, ProviderFactory},
};

pub(crate) struct SeaOrmConnectionFactory {
    pub database_url: String,
}

#[async_trait]
impl ProviderFactory for SeaOrmConnectionFactory {
    fn get_token(&self) -> String {
        std::any::type_name::<DatabaseConnection>().to_string()
    }

    async fn build(
        &self,
        _deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>> {
        let db = Database::connect(&self.database_url)
            .await
            .unwrap_or_else(|e| panic!("SeaORM: failed to connect to '{}': {e}", self.database_url));

        Arc::new(Box::new(SeaOrmConnectionProvider {
            db: Mutex::new(Some(db)),
        }))
    }
}

struct SeaOrmConnectionProvider {
    // Option so close() can take ownership on shutdown; Mutex for &self access.
    db: Mutex<Option<DatabaseConnection>>,
}

#[async_trait]
impl Provider for SeaOrmConnectionProvider {
    fn get_token(&self) -> String {
        std::any::type_name::<DatabaseConnection>().to_string()
    }

    fn get_token_factory(&self) -> String {
        std::any::type_name::<DatabaseConnection>().to_string()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send> {
        // DatabaseConnection is Clone — it wraps a connection pool internally.
        let db = self.db.lock().as_ref().expect("database already closed").clone();
        Box::new(db)
    }

    async fn on_application_shutdown(&self, _signal: Option<String>) {
        let db = self.db.lock().take();
        if let Some(db) = db {
            if let Err(e) = db.close().await {
                tracing::error!(error = %e, "SeaORM: error closing database connection");
            }
        }
    }
}
