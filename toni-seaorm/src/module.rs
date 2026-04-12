use sea_orm::DatabaseConnection;
use toni::DynamicModule;

use crate::connection::SeaOrmConnectionFactory;

pub struct SeaOrmModule;

impl SeaOrmModule {
    /// Register a database connection for the entire application.
    ///
    /// Returns a global `DynamicModule` that provides `DatabaseConnection` to every
    /// module without requiring explicit imports. Import this once in your root module:
    ///
    /// ```ignore
    /// #[module(imports: [SeaOrmModule::for_root(env!("DATABASE_URL"))])]
    /// pub struct AppModule;
    /// ```
    ///
    /// Then inject `DatabaseConnection` anywhere:
    ///
    /// ```ignore
    /// #[injectable(pub struct UserService {
    ///     db: DatabaseConnection,
    /// })]
    /// impl UserService {
    ///     pub async fn find_all(&self) -> Result<Vec<user::Model>, DbErr> {
    ///         user::Entity::find().all(&self.db).await
    ///     }
    /// }
    /// ```
    pub fn for_root(database_url: impl Into<String>) -> DynamicModule {
        DynamicModule::builder("SeaOrmModule")
            .provider(SeaOrmConnectionFactory {
                database_url: database_url.into(),
            })
            .export::<DatabaseConnection>()
            .global()
            .build()
    }
}
