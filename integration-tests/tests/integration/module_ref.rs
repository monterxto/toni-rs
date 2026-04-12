use toni::*;

// Test providers
#[injectable(pub struct DatabaseService {
    pub connection_string: String,
})]

impl DatabaseService {
    pub fn new() -> Self {
        Self {
            connection_string: "postgres://localhost:5432".to_string(),
        }
    }
}

#[injectable(pub struct CacheService {
    pub host: String,
})]

impl CacheService {
    pub fn new() -> Self {
        Self {
            host: "redis://localhost:6379".to_string(),
        }
    }
}

// Test service that uses ModuleRef for dynamic resolution
#[injectable(pub struct PluginLoader {
    #[inject]
    module_ref: ModuleRef,
})]

impl PluginLoader {
    /// Test strict mode (default) - only searches current module
    pub async fn load_service_strict(&self) -> Option<DatabaseService> {
        self.module_ref.get::<DatabaseService>().await.ok()
    }

    /// Test global mode - searches current module first, then globally
    pub async fn load_service_global(&self) -> Option<CacheService> {
        self.module_ref.get::<CacheService>().global().await.ok()
    }

    /// Test strict mode should fail for non-local service
    pub async fn load_cache_strict(&self) -> Option<CacheService> {
        self.module_ref.get::<CacheService>().await.ok()
    }

    /// Test token-based resolution with strict mode
    pub async fn load_by_token_strict(&self, token: &str) -> Option<DatabaseService> {
        self.module_ref
            .get_by_token::<DatabaseService>(token)
            .await
            .ok()
    }

    /// Get current module name
    pub fn current_module(&self) -> String {
        self.module_ref.current_module().to_string()
    }
}

// Module 1 - contains DatabaseService and PluginLoader
#[module(
    providers: [DatabaseService, PluginLoader],
    exports: [DatabaseService, PluginLoader],
)]
impl Module1 {}

// Module 2 - contains CacheService (global module)
#[module(
    providers: [CacheService],
    exports: [CacheService],
    global: true,
)]
impl Module2 {}

// Root module - imports both modules
#[module(
    imports: [Module1, Module2],
)]
impl AppModule {}

#[tokio::test]
async fn test_module_ref_strict_mode() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    // Get PluginLoader from Module1
    let plugin_loader = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Test strict mode - should successfully get DatabaseService from same module
    let db_service = plugin_loader
        .load_service_strict()
        .await
        .expect("Should resolve DatabaseService in strict mode");

    assert_eq!(db_service.connection_string, "postgres://localhost:5432");
}

#[tokio::test]
async fn test_module_ref_global_mode() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    let plugin_loader = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Test global mode - should find CacheService from global module
    let cache_service = plugin_loader
        .load_service_global()
        .await
        .expect("Should resolve CacheService in global mode");

    assert_eq!(cache_service.host, "redis://localhost:6379");
}

#[tokio::test]
async fn test_module_ref_strict_mode_fails_for_non_local_provider() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    let plugin_loader = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Test strict mode - should FAIL to get CacheService (it's in a different module)
    let result = plugin_loader.load_cache_strict().await;

    assert!(
        result.is_none(),
        "Should fail to resolve CacheService in strict mode (different module)"
    );
}

#[tokio::test]
async fn test_module_ref_token_based_resolution() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    let plugin_loader = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Test token-based resolution with type name
    let db_token = std::any::type_name::<DatabaseService>();
    let db_service = plugin_loader
        .load_by_token_strict(db_token)
        .await
        .expect("Should resolve DatabaseService by token");

    assert_eq!(db_service.connection_string, "postgres://localhost:5432");
}

#[tokio::test]
async fn test_module_ref_current_module() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    let plugin_loader = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Test current_module() method
    let module_name = plugin_loader.current_module();
    assert_eq!(module_name, "Module1", "Should return correct module name");
}

#[tokio::test]
async fn test_module_ref_singleton_behavior() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    let plugin_loader1 = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    let plugin_loader2 = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Both should point to the same module
    assert_eq!(
        plugin_loader1.current_module(),
        plugin_loader2.current_module(),
        "ModuleRef should be singleton per module"
    );

    // Both should resolve the same service instance (since DatabaseService is singleton)
    let db1 = plugin_loader1
        .load_service_strict()
        .await
        .expect("Should resolve");
    let db2 = plugin_loader2
        .load_service_strict()
        .await
        .expect("Should resolve");

    assert_eq!(db1.connection_string, db2.connection_string);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_module_ref_works_from_any_thread() {
    let app = ToniFactory::create(AppModule::module_definition()).await;

    let plugin_loader = app
        .get::<PluginLoader>()
        .await
        .expect("PluginLoader should be available");

    // Spawn onto a fresh OS thread — tokio's multi-threaded runtime can poll futures
    // on any worker, so ModuleRef::get() must work regardless of which thread calls it.
    let result = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(plugin_loader.load_service_strict())
    })
    .join()
    .expect("thread should not panic");

    assert!(
        result.is_some(),
        "ModuleRef::get should work from any thread, not just the initialization thread"
    );
}
