//! # TONI CUSTOM EXTRACTORS: GUIDE FOR NESTJS DEVELOPERS
//!
//! This example demonstrates how to create custom extractors in Toni,
//! which are equivalent to NestJS's `createParamDecorator()`.
//!
//! ## NestJS vs Toni Comparison:
//!
//! | Concept | NestJS | Toni |
//! |---------|--------|------|
//! | Define custom extractor | `createParamDecorator()` | `impl FromRequest` trait |
//! | Use in handler | `@CurrentUser()` decorator | `CurrentUser(user): CurrentUser` |
//! | Error handling | Throw exceptions | Return `Result<T, E>` |
//! | Type safety | Runtime (TypeScript) | Compile-time (Rust) |
//! | Performance | Reflection overhead | Zero-cost abstraction |
//!
//! ## Where Are Custom Decorators Used in NestJS?
//!
//! ### 1. **Method Parameters** (Most Common - createParamDecorator)
//! ```typescript
//! @Get('profile')
//! getProfile(@CurrentUser() user: User) { ... }
//! ```
//!
//! ### 2. **Route Method Decorators** (SetMetadata + Guards)
//! ```typescript
//! @Roles('admin')  // Custom method decorator
//! @Get('admin')
//! adminOnly() { ... }
//! ```
//! **Toni Equivalent:** Use `#[set_metadata(YourType { ... })]` on route + custom guards that
//! read metadata via `context.route_metadata().get::<YourType>()`
//!
//! ### 3. **Class Decorators** (Apply to all methods)
//! ```typescript
//! @UseGuards(AuthGuard)  // Applied to all controller methods
//! @Controller('users')
//! export class UserController { ... }
//! ```
//! **Toni Equivalent:** Controller-level `#[use_guards]`, `#[use_interceptors]`
//!
//! ### 4. **Property Decorators** (Dependency Injection)
//! ```typescript
//! @Injectable()
//! export class UserService {
//!   @InjectRepository(User)
//!   private userRepo: Repository<User>;
//! }
//! ```
//! **Toni Equivalent:** Field injection in `#[controller]` or `#[injectable]`
//!
//! ## This Example Focuses On: Parameter Extractors (createParamDecorator)
//!
//! We'll implement common NestJS custom decorator patterns:
//! - @CurrentUser() - Extract authenticated user from JWT
//! - @ApiKey() - Extract and validate API keys
//! - @ClientIp() - Extract client IP address
//! - @Cookies() - Parse and extract cookies
//! - @BearerToken() - Extract raw JWT token
//! - @RequestId() - Extract or generate request ID for tracing
//! - @UserAgent() - Extract user agent string
//! - Composition - Combine multiple extractors
//! - Option<T> - Optional authentication/data extraction

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use toni::extractors::FromRequest;
use toni::extractors::Json;
use toni::http_helpers::Body as ToniBody;
use toni::http_helpers::HttpRequest;
use toni::{controller, get, module, post};

// ============================================================================
// SECTION 1: AUTHENTICATION EXTRACTORS
// ============================================================================

/// ## 1. CurrentUser Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const CurrentUser = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return request.user;
///   },
/// );
/// ```
///
/// This is the most common custom decorator pattern - extracting the
/// authenticated user from the request after an auth guard/middleware
/// has validated the JWT and attached user data.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub roles: Vec<String>,
}

/// Custom extractor wrapping User
#[derive(Debug, Clone)]
pub struct CurrentUser(pub User);

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    ExpiredToken,
    Unauthorized,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "Authorization header missing"),
            AuthError::InvalidToken => write!(f, "Invalid JWT token format"),
            AuthError::ExpiredToken => write!(f, "JWT token has expired"),
            AuthError::Unauthorized => write!(f, "Insufficient permissions"),
        }
    }
}

impl std::error::Error for AuthError {}

impl FromRequest for CurrentUser {
    type Error = AuthError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let auth_header = req.header("authorization").ok_or(AuthError::MissingToken)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidToken)?;

        // In real app, use jsonwebtoken crate for JWT decoding
        let user = decode_jwt_token(token)?;

        Ok(CurrentUser(user))
    }
}

// Mock JWT decoder for example purposes
fn decode_jwt_token(token: &str) -> Result<User, AuthError> {
    if token.is_empty() {
        return Err(AuthError::InvalidToken);
    }

    if token == "expired" {
        return Err(AuthError::ExpiredToken);
    }

    Ok(User {
        id: "user123".to_string(),
        email: "user@example.com".to_string(),
        name: "John Doe".to_string(),
        roles: vec!["user".to_string()],
    })
}

/// ## 2. BearerToken Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const BearerToken = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     const auth = request.headers.authorization;
///     return auth?.replace('Bearer ', '');
///   },
/// );
/// ```
///
/// Extracts the raw JWT token without decoding it.
/// Useful for passing tokens to external services.

#[derive(Debug, Clone)]
pub struct BearerToken(pub String);

#[derive(Debug)]
pub enum TokenError {
    Missing,
    InvalidFormat,
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenError::Missing => write!(f, "Authorization header missing"),
            TokenError::InvalidFormat => write!(f, "Authorization header must be 'Bearer <token>'"),
        }
    }
}

impl std::error::Error for TokenError {}

impl FromRequest for BearerToken {
    type Error = TokenError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let auth_header = req.header("authorization").ok_or(TokenError::Missing)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(TokenError::InvalidFormat)?
            .to_string();

        Ok(BearerToken(token))
    }
}

/// ## 3. ApiKey Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const ApiKey = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return request.headers['x-api-key'];
///   },
/// );
/// ```
///
/// Extracts API key from custom header.
/// Common for public APIs with key-based authentication.

#[derive(Debug, Clone)]
pub struct ApiKey(pub String);

#[derive(Debug)]
pub enum ApiKeyError {
    Missing,
    Invalid,
}

impl fmt::Display for ApiKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiKeyError::Missing => write!(f, "X-API-Key header is required"),
            ApiKeyError::Invalid => {
                write!(f, "Invalid API key format (must be at least 32 characters)")
            }
        }
    }
}

impl std::error::Error for ApiKeyError {}

impl FromRequest for ApiKey {
    type Error = ApiKeyError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let key = req
            .header("x-api-key")
            .ok_or(ApiKeyError::Missing)?
            .to_string();

        // Validate key format
        if key.len() < 32 {
            return Err(ApiKeyError::Invalid);
        }

        Ok(ApiKey(key))
    }
}

// ============================================================================
// SECTION 2: REQUEST METADATA EXTRACTORS
// ============================================================================

/// ## 4. ClientIp Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const ClientIp = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return request.headers['x-forwarded-for']?.split(',')[0] || request.ip;
///   },
/// );
/// ```
///
/// Extracts client IP, respecting X-Forwarded-For for proxies.
/// Useful for rate limiting, geolocation, or security logging.

#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

#[derive(Debug)]
pub struct IpError(String);

impl fmt::Display for IpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cannot extract IP address: {}", self.0)
    }
}

impl std::error::Error for IpError {}

impl FromRequest for ClientIp {
    type Error = IpError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Check X-Forwarded-For first (proxy/load balancer support)
        if let Some(forwarded) = req.header("x-forwarded-for") {
            if let Some(ip) = forwarded.split(',').next() {
                return Ok(ClientIp(ip.trim().to_string()));
            }
        }

        // Check X-Real-IP (nginx)
        if let Some(real_ip) = req.header("x-real-ip") {
            return Ok(ClientIp(real_ip.to_string()));
        }

        // Fallback (in real app, you'd extract from socket connection)
        Err(IpError("No IP address headers found".to_string()))
    }
}

/// ## 5. UserAgent Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const UserAgent = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return request.headers['user-agent'];
///   },
/// );
/// ```
///
/// Extracts User-Agent string for analytics or compatibility checks.

#[derive(Debug, Clone)]
pub struct UserAgent(pub String);

#[derive(Debug)]
pub struct UserAgentError(String);

impl fmt::Display for UserAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UserAgentError {}

impl FromRequest for UserAgent {
    type Error = UserAgentError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let ua = req
            .header("user-agent")
            .ok_or_else(|| UserAgentError("User-Agent header missing".to_string()))?
            .to_string();

        Ok(UserAgent(ua))
    }
}

/// ## 6. RequestId Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const RequestId = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return request.headers['x-request-id'] || generateRequestId();
///   },
/// );
/// ```
///
/// Extracts or generates request ID for distributed tracing.
/// Used with logging middleware for request correlation.

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

#[derive(Debug)]
pub struct RequestIdError;

impl fmt::Display for RequestIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Request ID extraction failed")
    }
}

impl std::error::Error for RequestIdError {}

impl FromRequest for RequestId {
    type Error = RequestIdError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Check existing header
        if let Some(id) = req.header("x-request-id") {
            return Ok(RequestId(id.to_string()));
        }

        // Generate new ID (in real app, use uuid crate)
        let id = format!(
            "req_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        Ok(RequestId(id))
    }
}

// ============================================================================
// SECTION 3: COOKIE AND HEADER EXTRACTORS
// ============================================================================

/// ## 7. Cookies Extractor
///
/// NestJS equivalent:
/// ```typescript
/// export const Cookies = createParamDecorator(
///   (data: string | undefined, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return data ? request.cookies?.[data] : request.cookies;
///   },
/// );
/// ```
///
/// Parses Cookie header into a HashMap.
/// In NestJS, you'd use cookie-parser middleware first.

#[derive(Debug, Clone)]
pub struct Cookies(pub HashMap<String, String>);

#[derive(Debug)]
pub struct CookieError(String);

impl fmt::Display for CookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cookie error: {}", self.0)
    }
}

impl std::error::Error for CookieError {}

impl FromRequest for Cookies {
    type Error = CookieError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let cookie_header = req
            .header("cookie")
            .ok_or_else(|| CookieError("No cookies present".to_string()))?;

        let mut cookies = HashMap::new();

        // Parse cookies: "key1=value1; key2=value2"
        for pair in cookie_header.split(';') {
            let mut parts = pair.trim().splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                cookies.insert(key.to_string(), value.to_string());
            }
        }

        Ok(Cookies(cookies))
    }
}

/// ## 8. SingleCookie Extractor (with name parameter)
///
/// This shows how to create parameterized extractors.
/// In NestJS: `@Cookies('sessionId')`
/// In Toni: We need a different approach since we can't pass parameters to extractors directly.
///
/// Solution: Create specific extractor types or use the Cookies extractor and extract manually.

#[derive(Debug, Clone)]
pub struct SessionCookie(pub String);

#[derive(Debug)]
pub struct SessionCookieError(String);

impl fmt::Display for SessionCookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session cookie error: {}", self.0)
    }
}

impl std::error::Error for SessionCookieError {}

impl FromRequest for SessionCookie {
    type Error = SessionCookieError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let cookie_header = req
            .header("cookie")
            .ok_or_else(|| SessionCookieError("No cookies present".to_string()))?;

        for pair in cookie_header.split(';') {
            let mut parts = pair.trim().splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                if key == "session_id" || key == "sessionId" {
                    return Ok(SessionCookie(value.to_string()));
                }
            }
        }

        Err(SessionCookieError(
            "session_id cookie not found".to_string(),
        ))
    }
}

// ============================================================================
// SECTION 4: COMPOSITE EXTRACTORS
// ============================================================================

/// ## 9. AuthContext - Combining Multiple Extractions
///
/// NestJS pattern:
/// ```typescript
/// export const AuthContext = createParamDecorator(
///   (data: unknown, ctx: ExecutionContext) => {
///     const request = ctx.switchToHttp().getRequest();
///     return {
///       user: request.user,
///       ip: request.ip,
///       userAgent: request.headers['user-agent'],
///     };
///   },
/// );
/// ```
///
/// This demonstrates extracting multiple pieces of data into one context object.

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user: User,
    pub ip: String,
    pub user_agent: String,
}

#[derive(Debug)]
pub struct AuthContextError(String);

impl fmt::Display for AuthContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Auth context error: {}", self.0)
    }
}

impl std::error::Error for AuthContextError {}

impl FromRequest for AuthContext {
    type Error = AuthContextError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let CurrentUser(user) = CurrentUser::from_request(req)
            .map_err(|e| AuthContextError(format!("User extraction failed: {}", e)))?;

        let ClientIp(ip) = ClientIp::from_request(req)
            .map_err(|e| AuthContextError(format!("IP extraction failed: {}", e)))?;

        let UserAgent(user_agent) = UserAgent::from_request(req)
            .map_err(|e| AuthContextError(format!("User-Agent extraction failed: {}", e)))?;

        Ok(AuthContext {
            user,
            ip,
            user_agent,
        })
    }
}

// ============================================================================
// SECTION 5: CONTROLLERS DEMONSTRATING USAGE
// ============================================================================

/// Controller showing basic extractor usage
#[controller(
    "/auth",
    pub struct AuthController {}
)]
impl AuthController {
    /// Example 1: Extract authenticated user
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('profile')
    /// getProfile(@CurrentUser() user: User) {
    ///   return user;
    /// }
    /// ```
    #[get("/profile")]
    fn get_profile(&self, CurrentUser(user): CurrentUser) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "id": user.id,
            "email": user.email,
            "name": user.name,
            "roles": user.roles
        }))
    }

    /// Example 2: Extract bearer token
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('token')
    /// getToken(@BearerToken() token: string) {
    ///   return { token };
    /// }
    /// ```
    #[get("/token")]
    fn get_token(&self, BearerToken(token): BearerToken) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "token": token,
            "length": token.len()
        }))
    }
}

/// Controller showing API key authentication
#[controller(
    "/api",
    pub struct ApiController {}
)]
impl ApiController {
    /// Example 3: Extract and validate API key
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('data')
    /// getData(@ApiKey() key: string) {
    ///   return { message: 'Authenticated with API key' };
    /// }
    /// ```
    #[get("/data")]
    fn get_data(&self, ApiKey(key): ApiKey) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "message": "Authenticated with API key",
            "key_prefix": &key[..8]
        }))
    }
}

/// Controller showing request metadata extraction
#[controller(
    "/metadata",
    pub struct MetadataController {}
)]
impl MetadataController {
    /// Example 4: Extract client IP
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('ip')
    /// getIp(@ClientIp() ip: string) {
    ///   return { ip };
    /// }
    /// ```
    #[get("/ip")]
    fn get_ip(&self, ClientIp(ip): ClientIp) -> ToniBody {
        ToniBody::text(format!("Your IP: {}", ip))
    }

    /// Example 5: Extract user agent
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('user-agent')
    /// getUserAgent(@UserAgent() ua: string) {
    ///   return { userAgent: ua };
    /// }
    /// ```
    #[get("/user-agent")]
    fn get_user_agent(&self, UserAgent(ua): UserAgent) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "userAgent": ua
        }))
    }

    /// Example 6: Extract request ID for tracing
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('trace')
    /// trace(@RequestId() requestId: string) {
    ///   return { requestId };
    /// }
    /// ```
    #[get("/trace")]
    fn trace(&self, RequestId(id): RequestId) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "requestId": id,
            "message": "Use this ID for request tracing"
        }))
    }
}

/// Controller showing cookie extraction
#[controller(
    "/session",
    pub struct SessionController {}
)]
impl SessionController {
    /// Example 7: Extract all cookies
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('cookies')
    /// getCookies(@Cookies() cookies: Record<string, string>) {
    ///   return cookies;
    /// }
    /// ```
    #[get("/cookies")]
    fn get_cookies(&self, Cookies(cookies): Cookies) -> ToniBody {
        ToniBody::json(serde_json::json!(cookies))
    }

    /// Example 8: Extract specific cookie
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('session')
    /// getSession(@Cookies('sessionId') sessionId: string) {
    ///   return { sessionId };
    /// }
    /// ```
    #[get("/session")]
    fn get_session(&self, SessionCookie(session_id): SessionCookie) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "sessionId": session_id
        }))
    }
}

/// Controller showing optional extraction
#[controller(
    "/optional",
    pub struct OptionalController {}
)]
impl OptionalController {
    /// Optional authentication - returns None when extraction fails instead of 400 error
    #[get("/feed")]
    fn get_feed(&self, user: Option<CurrentUser>) -> ToniBody {
        if let Some(CurrentUser(user)) = user {
            ToniBody::json(serde_json::json!({
                "type": "personalized",
                "message": format!("Welcome back, {}!", user.name),
                "items": ["Based on your interests", "Recommended for you"]
            }))
        } else {
            ToniBody::json(serde_json::json!({
                "type": "public",
                "message": "Sign in for personalized content",
                "items": ["Popular posts", "Trending articles"]
            }))
        }
    }

    /// Multiple optional extractors - supports JWT, API key, or public access
    #[get("/data")]
    fn get_data(&self, user: Option<CurrentUser>, api_key: Option<ApiKey>) -> ToniBody {
        if let Some(CurrentUser(user)) = user {
            return ToniBody::json(serde_json::json!({
                "auth": "jwt",
                "userId": user.id
            }));
        }

        if let Some(ApiKey(key)) = api_key {
            return ToniBody::json(serde_json::json!({
                "auth": "apiKey",
                "keyPrefix": &key[..8]
            }));
        }

        ToniBody::json(serde_json::json!({
            "auth": "none",
            "message": "Public access (limited)"
        }))
    }
}

/// Controller showing composite extraction
#[controller(
    "/advanced",
    pub struct AdvancedController {}
)]
impl AdvancedController {
    /// Example 9: Multiple extractors in one handler
    ///
    /// NestJS:
    /// ```typescript
    /// @Post('audit')
    /// audit(
    ///   @CurrentUser() user: User,
    ///   @ClientIp() ip: string,
    ///   @Body() data: AuditDto,
    /// ) {
    ///   return { user, ip, data };
    /// }
    /// ```
    #[post("/audit")]
    fn audit(
        &self,
        CurrentUser(user): CurrentUser,
        ClientIp(ip): ClientIp,
        Json(data): Json<AuditData>,
    ) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "user": {
                "id": user.id,
                "email": user.email
            },
            "ip": ip,
            "action": data.action,
            "timestamp": data.timestamp
        }))
    }

    /// Example 10: Composite extractor (all-in-one)
    ///
    /// NestJS:
    /// ```typescript
    /// @Get('context')
    /// getContext(@AuthContext() ctx: AuthContext) {
    ///   return ctx;
    /// }
    /// ```
    #[get("/context")]
    fn get_context(&self, context: AuthContext) -> ToniBody {
        ToniBody::json(serde_json::json!({
            "user": {
                "id": context.user.id,
                "email": context.user.email
            },
            "ip": context.ip,
            "userAgent": context.user_agent
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct AuditData {
    action: String,
    timestamp: i64,
}

// ============================================================================
// SECTION 6: MODULE AND MAIN
// ============================================================================

#[module(
    imports: [],
    controllers: [
        AuthController,
        ApiController,
        MetadataController,
        SessionController,
        OptionalController,
        AdvancedController
    ],
    providers: [],
    exports: []
)]
pub struct AppModule;

#[tokio::main]
async fn main() {
    println!("Custom Extractors Example");
    println!("=====================================");
    println!();
    println!("This example demonstrates custom extractors in Toni,");
    println!("which are equivalent to NestJS's createParamDecorator().");
    println!();
    println!("Available endpoints:");
    println!();
    println!("   Authentication:");
    println!("   GET  /auth/profile      - Extract current user from JWT");
    println!("   GET  /auth/token        - Extract bearer token");
    println!();
    println!("   API Key:");
    println!("   GET  /api/data          - Validate API key");
    println!();
    println!("   Metadata:");
    println!("   GET  /metadata/ip       - Extract client IP");
    println!("   GET  /metadata/user-agent - Extract user agent");
    println!("   GET  /metadata/trace    - Extract/generate request ID");
    println!();
    println!("   Cookies:");
    println!("   GET  /session/cookies   - Extract all cookies");
    println!("   GET  /session/session   - Extract session cookie");
    println!();
    println!("   Optional:");
    println!("   GET  /optional/feed     - Optional auth (personalized vs public)");
    println!("   GET  /optional/data     - Multiple optional extractors");
    println!();
    println!("   Advanced:");
    println!("   POST /advanced/audit    - Multiple extractors");
    println!("   GET  /advanced/context  - Composite extractor");
    println!();
    println!("Test with curl:");
    println!();
    println!(r#"   curl -H "Authorization: Bearer test-token" http://localhost:3000/auth/profile"#);
    println!(
        r#"   curl -H "X-API-Key: 12345678901234567890123456789012" http://localhost:3000/api/data"#
    );
    println!(r#"   curl http://localhost:3000/metadata/ip"#);
    println!();
    println!("Key Takeaway:");
    println!("   Toni's FromRequest trait = NestJS's createParamDecorator()");
    println!("   - More type-safe (compile-time errors)");
    println!("   - More explicit (no hidden magic)");
    println!("   - More composable (wrap extractors)");
    println!();
    println!("Starting server on http://localhost:3000");
    println!("=====================================");
    println!();

    use toni::ToniFactory;
    use toni_axum::AxumAdapter;

    let mut app = ToniFactory::create(AppModule::module_definition()).await;
    app.use_http_adapter(AxumAdapter::new("127.0.0.1", 3000)).unwrap();
    app.start().await;
}
