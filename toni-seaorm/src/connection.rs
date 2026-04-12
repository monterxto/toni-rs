use std::{any::Any, sync::Arc};

use async_trait::async_trait;
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

        Arc::new(Box::new(SeaOrmConnectionProvider { db }))
    }
}

struct SeaOrmConnectionProvider {
    db: DatabaseConnection,
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
        Box::new(self.db.clone())
    }
}
