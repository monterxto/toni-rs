use async_trait::async_trait;
use juniper::{graphql_object, EmptySubscription, FieldResult, RootNode};
use std::sync::Arc;
use toni::{injectable, module, toni_factory::ToniFactory, HttpAdapter, HttpRequest};
use toni_axum::AxumAdapter;
use toni_juniper::{ContextBuilder, GraphQLModule};

// ============================================================================
// Domain Models
// ============================================================================

#[derive(Clone)]
struct User {
    id: i32,
    username: String,
    email: String,
}

#[graphql_object(context = GraphQLContext)]
impl User {
    fn id(&self) -> i32 {
        self.id
    }

    fn username(&self) -> &str {
        &self.username
    }

    fn email(&self) -> &str {
        &self.email
    }
}

// ============================================================================
// Services (Toni DI)
// ============================================================================

/// Authentication service (injectable via Toni DI)
#[injectable(pub struct _AuthService;)]
impl _AuthService {
    fn verify_token(&self, req: &toni::RequestPart) -> Option<User> {
        // In a real app, verify JWT token from headers
        let auth_value = req
            .headers
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
    fn find_user(&self, id: i32) -> Option<User> {
        // In a real app, query the database
        Some(User {
            id,
            username: format!("user_{}", id),
            email: format!("user{}@example.com", id),
        })
    }

    fn list_users(&self) -> Vec<User> {
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
// GraphQL Context (with DI!)
// ============================================================================

/// GraphQL context with authenticated user and DI services
#[derive(Clone)]
pub struct GraphQLContext {
    user: Option<User>,
    database_service: Arc<_DatabaseService>,
}

impl juniper::Context for GraphQLContext {}

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
    type Context = GraphQLContext;

    async fn build(&self, req: &toni::RequestPart) -> Self::Context {
        GraphQLContext {
            user: self.auth_service.verify_token(req),
            database_service: Arc::new(self.database_service.clone()),
        }
    }
}

// ============================================================================
// GraphQL Schema
// ============================================================================

struct Query;

#[graphql_object(context = GraphQLContext)]
impl Query {
    /// Get the current authenticated user
    fn me(context: &GraphQLContext) -> FieldResult<User> {
        context
            .user
            .clone()
            .ok_or_else(|| "Not authenticated".into())
    }

    /// Get a user by ID (requires authentication)
    fn user(context: &GraphQLContext, id: i32) -> FieldResult<User> {
        // Check if authenticated
        if context.user.is_none() {
            return Err("Not authenticated".into());
        }

        // Query database
        context
            .database_service
            .find_user(id)
            .ok_or_else(|| "User not found".into())
    }

    /// List all users (requires authentication)
    fn users(context: &GraphQLContext) -> FieldResult<Vec<User>> {
        // Check if authenticated
        if context.user.is_none() {
            return Err("Not authenticated".into());
        }

        // Query database
        Ok(context.database_service.list_users())
    }

    /// Public query (no auth required)
    fn hello() -> &'static str {
        "Hello, world!"
    }
}

struct Mutation;

#[graphql_object(context = GraphQLContext)]
impl Mutation {
    /// Update user profile (requires authentication)
    fn update_profile(
        context: &GraphQLContext,
        username: String,
        email: String,
    ) -> FieldResult<User> {
        // Get current user
        let user = context.user.as_ref().ok_or_else(|| "Not authenticated")?;

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
) -> GraphQLModule<Query, Mutation, EmptySubscription<GraphQLContext>, _GraphQLContextBuilder> {
    let schema = RootNode::new(Query, Mutation, EmptySubscription::new());

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

    app.use_http_adapter(AxumAdapter::new(), 3000, "127.0.0.1")
        .unwrap();

    app.start().await;
}
