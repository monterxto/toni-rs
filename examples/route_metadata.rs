//! Route Metadata Example
//!
//! Demonstrates how to use #[set_metadata(...)] to pass route-level configuration
//! to guards, interceptors, and other enhancers.
//!
//! This is Toni's equivalent to NestJS's @SetMetadata() + Reflector pattern,
//! but type-safe and without runtime reflection.
//!
//! ## How It Works
//!
//! 1. Define metadata types (any Clone + Send + Sync + 'static type)
//! 2. Attach metadata to routes with `#[set_metadata(YourType { ... })]`
//! 3. Guards/interceptors read via `context.route_metadata().get::<YourType>()`

use toni::{
    controller, get, http_helpers::Body as ToniBody, module, set_metadata, traits_helpers::Guard,
    use_guards, Context,
};

// ============================================================================
// Metadata Types
// ============================================================================

/// Required roles to access a route
#[derive(Clone)]
pub struct Roles(pub &'static [&'static str]);

/// Rate limiting configuration
#[derive(Clone)]
pub struct RateLimit {
    pub max_requests: u32,
    pub window_secs: u32,
}

/// Marks a route as publicly accessible (bypasses auth)
#[derive(Clone)]
pub struct Public;

// ============================================================================
// Guards That Read Metadata
// ============================================================================

pub struct RolesGuard;

impl Guard for RolesGuard {
    fn can_activate(&self, context: &Context) -> bool {
        let metadata = context.route_metadata();

        // Public routes bypass role checks
        if metadata.get::<Public>().is_some() {
            return true;
        }

        // No roles specified = allow all
        let Some(Roles(required)) = metadata.get::<Roles>() else {
            return true;
        };

        // In production: extract user from JWT/session and check roles
        let req = context.take_request();
        let user_role = req.header("x-user-role").unwrap_or("guest");

        required.iter().any(|&r| r == user_role)
    }
}

pub struct RateLimitGuard;

impl Guard for RateLimitGuard {
    fn can_activate(&self, context: &Context) -> bool {
        let metadata = context.route_metadata();

        let Some(RateLimit {
            max_requests,
            window_secs,
        }) = metadata.get::<RateLimit>()
        else {
            return true;
        };

        // In production: check rate limit against Redis/in-memory store
        println!(
            "Rate limit check: {} requests per {} seconds",
            max_requests, window_secs
        );

        true
    }
}

// ============================================================================
// Controller With Metadata
// ============================================================================

#[controller("/api", pub struct ApiController {})]
#[use_guards(RolesGuard{}, RateLimitGuard{})]
impl ApiController {
    /// Public health check - no auth required
    #[set_metadata(Public)]
    #[get("/health")]
    fn health(&self) -> ToniBody {
        ToniBody::Json(serde_json::json!({ "status": "ok" }))
    }

    /// User endpoint - any authenticated user
    #[set_metadata(Roles(&["user", "admin"]))]
    #[set_metadata(RateLimit { max_requests: 100, window_secs: 60 })]
    #[get("/profile")]
    fn profile(&self) -> ToniBody {
        ToniBody::Json(serde_json::json!({ "user": "current_user" }))
    }

    /// Admin only endpoint
    #[set_metadata(Roles(&["admin"]))]
    #[get("/admin/stats")]
    fn admin_stats(&self) -> ToniBody {
        ToniBody::Json(serde_json::json!({ "total_users": 1000 }))
    }

    /// Moderator or admin
    #[set_metadata(Roles(&["admin", "moderator"]))]
    #[get("/moderate")]
    fn moderate(&self) -> ToniBody {
        ToniBody::Json(serde_json::json!({ "queue": [] }))
    }
}

#[module(
    controllers: [ApiController],
    providers: [],
)]
pub struct AppModule;

fn main() {
    println!("Route Metadata Example");
    println!("======================");
    println!();
    println!("Available endpoints:");
    println!("  GET /api/health      - Public (no auth)");
    println!("  GET /api/profile     - user or admin role, rate limited");
    println!("  GET /api/admin/stats - admin role only");
    println!("  GET /api/moderate    - admin or moderator role");
    println!();
    println!("Test with:");
    println!("  curl http://localhost:3000/api/health");
    println!("  curl -H 'x-user-role: admin' http://localhost:3000/api/admin/stats");
    println!("  curl -H 'x-user-role: user' http://localhost:3000/api/profile");
    println!();

    use toni::ToniFactory;
    use toni_axum::AxumAdapter;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let adapter = AxumAdapter::new();
        let app = ToniFactory::create(AppModule::module_definition(), adapter).await;
        app.listen(3000, "127.0.0.1").await;
    });
}
