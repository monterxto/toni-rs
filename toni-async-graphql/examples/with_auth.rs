use async_trait::async_trait;
use std::sync::Arc;
use toni::{injectable, module, toni_factory::ToniFactory, HttpAdapter, HttpRequest};
use toni_async_graphql::{async_graphql::*, prelude::*, ContextBuilder};
use toni_axum::AxumAdapter;

// ============================================================================
// Domain Models
// ============================================================================

#[derive(SimpleObject, Clone)]
struct User {
    id: i32,
    username: String,
    email: String,
}

// ============================================================================
// Services (Toni DI)
// ============================================================================

/// Authentication service (injectable via Toni DI)
#[injectable(pub struct _AuthService;)]
impl _AuthService {
    fn verify_token(&self, req: &HttpRequest) -> Option<User> {
        // In a real app, verify JWT token from headers
        let auth_value = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())?;

        if auth_value.starts_with("Bearer valid-token") {
            Some(User {
                id: 1,
                username: "john_doe".to_string(),
                email: "john@example.com".to_string(),
            })
        } else {
            None
        }
    }
}

/// Database service (injectable via Toni DI)
#[injectable(pub struct _DatabaseService;)]
impl _DatabaseService {
    async fn find_user(&self, id: i32) -> Option<User> {
        // In a real app, query the database
        Some(User {
            id,
            username: format!("user_{}", id),
            email: format!("user{}@example.com", id),
        })
    }

    async fn list_users(&self) -> Vec<User> {
        // In a real app, query the database
        vec![
            User {
                id: 1,
                username: "alice".to_string(),
                email: "alice@example.com".to_string(),
            },
            User {
                id: 2,
                username: "bob".to_string(),
                email: "bob@example.com".to_string(),
            },
        ]
    }
}

// ============================================================================
// GraphQL Context Builder (with DI!)
// ============================================================================

/// Context builder that injects Toni services!
#[injectable(
    pub struct _GraphQLContextBuilder {
        #[inject]
        auth_service: _AuthService,
        #[inject]
        database_service: _DatabaseService,
    }
)]
#[async_trait]
impl ContextBuilder for _GraphQLContextBuilder {
    async fn build(&self, req: &HttpRequest) -> Data {
        let mut data = Data::default();

        // Add HTTP request to context
        data.insert(req.clone());

        // Add authenticated user (if valid token)
        if let Some(user) = self.auth_service.verify_token(req) {
            data.insert(user);
        }

        // Add database service to context (so resolvers can use it!)
        data.insert(Arc::new(self.database_service.clone()));

        data
    }
}

// ============================================================================
// GraphQL Schema
// ============================================================================

struct Query;

#[Object]
impl Query {
    /// Get the current authenticated user
    async fn me(&self, ctx: &Context<'_>) -> Result<User> {
        // Extract user from context (added by auth)
        ctx.data::<User>()
            .map(|u| u.clone())
            .map_err(|_| "Not authenticated".into())
    }

    /// Get a user by ID (requires authentication)
    async fn user(&self, ctx: &Context<'_>, id: i32) -> Result<User> {
        // Check if authenticated
        ctx.data::<User>().map_err(|_| "Not authenticated")?;

        // Get database service from context
        let db = ctx.data::<Arc<_DatabaseService>>()?;

        // Query database
        db.find_user(id)
            .await
            .ok_or_else(|| "User not found".into())
    }

    /// List all users (requires authentication)
    async fn users(&self, ctx: &Context<'_>) -> Result<Vec<User>> {
        // Check if authenticated
        ctx.data::<User>().map_err(|_| "Not authenticated")?;

        // Get database service from context
        let db = ctx.data::<Arc<_DatabaseService>>()?;

        // Query database
        Ok(db.list_users().await)
    }

    /// Public query (no auth required)
    async fn hello(&self) -> &str {
        "Hello, world!"
    }
}

struct Mutation;

#[Object]
impl Mutation {
    /// Update user profile (requires authentication)
    async fn update_profile(
        &self,
        ctx: &Context<'_>,
        username: String,
        email: String,
    ) -> Result<User> {
        // Get current user
        let user = ctx.data::<User>().map_err(|_| "Not authenticated")?;

        // In a real app, update the database
        Ok(User {
            id: user.id,
            username,
            email,
        })
    }
}

// ============================================================================
// App Module (with DI providers)
// ============================================================================

fn build_graphql_module(
) -> GraphQLModule<Query, Mutation, EmptySubscription, _GraphQLContextBuilder> {
    let schema = Schema::build(Query, Mutation, EmptySubscription).finish();

    // Create context builder (will be injected with services by Toni!)
    let context_builder = _GraphQLContextBuilder {
        auth_service: _AuthService {},
        database_service: _DatabaseService {},
    };

    GraphQLModule::for_root(schema, context_builder)
        .with_path("/graphql")
        .with_playground(true)
}

#[module(
    imports: [build_graphql_module()],
    controllers: [],
    providers: [_AuthService, _DatabaseService, _GraphQLContextBuilder],
    exports: []
)]
impl AppModule {}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("Starting GraphQL Auth example...");
    println!("\n🔑 Authentication:");
    println!("  - Add header: Authorization: Bearer valid-token");
    println!("  - Any other token will be unauthorized\n");

    println!("📍 Endpoints:");
    println!("  - GraphQL API: http://localhost:3000/graphql");
    println!("  - GraphQL Playground: http://localhost:3000/graphql (browser)\n");

    println!("🧪 Try these queries:");
    println!("\n1. Public query (no auth needed):");
    println!("   query {{ hello }}");

    println!("\n2. Get current user (requires auth):");
    println!("   query {{ me {{ id username email }} }}");

    println!("\n3. List users (requires auth):");
    println!("   query {{ users {{ id username email }} }}");

    println!("\n4. Update profile (requires auth):");
    println!("   mutation {{ updateProfile(username: \"newname\", email: \"new@example.com\") {{ id username email }} }}\n");

    // Create Toni app
    let mut app = ToniFactory::create(AppModule::module_definition()).await;

    app.use_http_adapter(AxumAdapter::new("127.0.0.1", 3000)).unwrap();

    app.start().await;
}
