//! # TONI PIPES: COMPLETE GUIDE FOR NESTJS DEVELOPERS
//!
//! This example demonstrates ALL pipe patterns in Toni compared to NestJS.
//!
//! ## Key Differences from NestJS:
//!
//! 1. **No Global Pipes**: Toni uses explicit `Validated<T>` instead of `app.useGlobalPipes()`
//!    - WHY: Rust's type system makes validation part of the signature, not runtime magic
//!    - BENEFIT: You SEE validation requirements in the function signature
//!
//! 2. **Extractors Replace Most Pipes**: `Path<i32>` = ParseIntPipe built-in
//!    - WHY: Serde does parsing automatically based on type
//!    - BENEFIT: No need for transformation pipes - types do the work
//!
//! 3. **Validation is Type-Level**: `Validated<Json<T>>` = ValidationPipe
//!    - WHY: Validator crate + derive macros = compile-time safety
//!    - BENEFIT: Can't forget to validate - compiler enforces it
//!
//! 4. **No @UsePipes() on Controllers**: Each handler declares its own requirements
//!    - WHY: No controller-level inheritance - explicit per handler
//!    - BENEFIT: Clear intent, no hidden behavior
//!
//! ## What You'll Learn:
//!
//! ✅ Basic extractors (equivalent to @Body, @Query, @Param)
//! ✅ Validation (equivalent to ValidationPipe + class-validator)
//! ✅ Type parsing (equivalent to ParseIntPipe, ParseBoolPipe)
//! ✅ Optional values (equivalent to DefaultValuePipe)
//! ✅ Custom validation (equivalent to custom pipes)
//! ✅ Composition (equivalent to multiple @UsePipes)
//! ✅ Why NO global pipes (and what to use instead)

use serde::Deserialize;
use toni::{
    controller,
    extractors::{Json, Path, Query, Validated},
    get,
    http_helpers::Body as ToniBody,
    module, post, HttpAdapter,
};
use validator::Validate;

// ============================================================================
// SECTION 1: BASIC EXTRACTORS (No Pipes Needed!)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// @Post('users')
/// create(@Body() dto: CreateUserDto) {
///   return { id: 1, ...dto };
/// }
/// ```
///
/// Toni Version: Identical intent, but explicit type
#[derive(Debug, Deserialize)]
struct CreateUserDto {
    name: String,
    email: String,
}

/// NestJS Version:
/// ```typescript
/// @Get('users/:id')
/// findOne(@Param('id') id: string) {
///   return { id, name: 'John' };
/// }
/// ```
///
/// Toni Version: `Path` extractor = `@Param()`
#[derive(Debug, Deserialize)]
struct UserParams {
    id: String,
}

/// NestJS Version:
/// ```typescript
/// @Get('users')
/// findAll(@Query('page') page: string, @Query('limit') limit: string) {
///   return { page, limit };
/// }
/// ```
///
/// Toni Version: `Query` extractor = `@Query()`
#[derive(Debug, Deserialize)]
struct PaginationQuery {
    page: Option<u32>,
    limit: Option<u32>,
}

// ============================================================================
// SECTION 2: VALIDATION (Replaces ValidationPipe)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// // Global setup:
/// app.useGlobalPipes(new ValidationPipe());
///
/// // DTO with validation:
/// class CreateProductDto {
///   @IsString()
///   @MinLength(3)
///   name: string;
///
///   @IsNumber()
///   @Min(0)
///   price: number;
///
///   @IsEmail()
///   contactEmail: string;
/// }
///
/// // Controller:
/// @Post('products')
/// create(@Body() dto: CreateProductDto) {
///   // Validation happens automatically due to global pipe
///   return dto;
/// }
/// ```
///
/// Toni Version: Explicit `Validated<Json<T>>` wrapper
/// WHY DIFFERENT:
/// - NestJS: Validation is HIDDEN in global config
/// - Toni: Validation is VISIBLE in function signature
///
/// BENEFITS:
/// - See validation requirements immediately
/// - Can't forget to validate - it's in the type
/// - No runtime surprises
#[derive(Debug, Deserialize, Validate)]
struct CreateProductDto {
    #[validate(length(min = 3, message = "Name must be at least 3 characters"))]
    name: String,

    #[validate(range(min = 0.0, message = "Price cannot be negative"))]
    price: f64,

    #[validate(email(message = "Invalid email format"))]
    contact_email: String,
}

// ============================================================================
// SECTION 3: TYPE PARSING (Replaces ParseIntPipe, ParseBoolPipe)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// @Get('products/:id')
/// findOne(@Param('id', ParseIntPipe) id: number) {
///   // ParseIntPipe converts string → number, throws 400 if invalid
///   return { id, name: 'Product' };
/// }
/// ```
///
/// Toni Version: Just use `Path<i32>` - parsing is automatic!
/// WHY NO PIPE NEEDED:
/// - Serde deserializes based on type
/// - If path has "42", Path<i32> gives you 42
/// - If path has "abc", extraction fails with 400 automatically
///
/// THIS IS THE MAGIC: Types do the work!
#[derive(Debug, Deserialize)]
struct ProductIdParam {
    id: i32, // ← Automatically parsed from string!
}

/// NestJS Version:
/// ```typescript
/// @Get('search')
/// search(
///   @Query('active', ParseBoolPipe) active: boolean,
///   @Query('minPrice', ParseIntPipe) minPrice: number
/// ) {
///   return { active, minPrice };
/// }
/// ```
///
/// Toni Version: Types handle it
#[derive(Debug, Deserialize)]
struct SearchQuery {
    active: bool,   // ← "true" → true, "false" → false
    min_price: i32, // ← "100" → 100
}

// ============================================================================
// SECTION 4: OPTIONAL VALUES & DEFAULTS (Replaces DefaultValuePipe)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// @Get('items')
/// findAll(
///   @Query('page', new DefaultValuePipe(1), ParseIntPipe) page: number,
///   @Query('limit', new DefaultValuePipe(10), ParseIntPipe) limit: number
/// ) {
///   return { page, limit };
/// }
/// ```
///
/// Toni Version 1: Use Option<T> + unwrap_or in handler
/// Toni Version 2: Use serde default attribute
#[derive(Debug, Deserialize)]
struct ItemsQuery {
    page: Option<u32>,  // None if not provided
    limit: Option<u32>, // None if not provided
}

// Alternative: Default values at type level
#[derive(Debug, Deserialize)]
struct ItemsQueryWithDefaults {
    #[serde(default = "default_page")]
    page: u32,

    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_page() -> u32 {
    1
}
fn default_limit() -> u32 {
    10
}

// ============================================================================
// SECTION 5: CUSTOM VALIDATION (Replaces Custom Pipes)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// @Injectable()
/// export class TrimPipe implements PipeTransform {
///   transform(value: any) {
///     return typeof value === 'string' ? value.trim() : value;
///   }
/// }
///
/// @Post('comments')
/// create(@Body(TrimPipe) dto: CreateCommentDto) {
///   return dto;
/// }
/// ```
///
/// Toni Version: Custom serde deserializer
/// WHY: Transformation happens during extraction, not after
use serde::de::{self, Deserializer};

fn trim_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(s.trim().to_string())
}

#[derive(Debug, Deserialize, Validate)]
struct CreateCommentDto {
    #[serde(deserialize_with = "trim_string")]
    #[validate(length(min = 1, message = "Comment cannot be empty"))]
    text: String,

    #[serde(deserialize_with = "trim_string")]
    #[validate(length(min = 2, max = 50, message = "Author name must be 2-50 characters"))]
    author: String,
}

// ============================================================================
// SECTION 6: COMPLEX VALIDATION (Replaces Custom ValidationPipes)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// @Injectable()
/// export class PasswordMatchPipe implements PipeTransform {
///   transform(value: RegisterDto) {
///     if (value.password !== value.confirmPassword) {
///       throw new BadRequestException('Passwords do not match');
///     }
///     return value;
///   }
/// }
///
/// @Post('register')
/// register(@Body(PasswordMatchPipe) dto: RegisterDto) {
///   return { success: true };
/// }
/// ```
///
/// Toni Version: Custom validation in Validate trait
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_password_match"))]
struct RegisterDto {
    #[validate(email)]
    email: String,

    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    password: String,

    confirm_password: String,
}

fn validate_password_match(dto: &RegisterDto) -> Result<(), validator::ValidationError> {
    if dto.password != dto.confirm_password {
        return Err(validator::ValidationError::new("passwords_do_not_match"));
    }
    Ok(())
}

// ============================================================================
// SECTION 7: COMPOSITION (Replaces Multiple @UsePipes)
// ============================================================================

/// NestJS Version:
/// ```typescript
/// @Post('posts')
/// create(
///   @Body(TrimPipe, ValidationPipe) dto: CreatePostDto
/// ) {
///   // Pipes execute in order: TrimPipe → ValidationPipe
///   return dto;
/// }
/// ```
///
/// Toni Version: Composition through extractor nesting + custom deserializers
/// The execution order is:
/// 1. JSON extraction (Body or Json)
/// 2. Custom serde deserializers (trim_string)
/// 3. Validation (Validated wrapper)
#[derive(Debug, Deserialize, Validate)]
struct CreatePostDto {
    #[serde(deserialize_with = "trim_string")]
    #[validate(length(min = 5, max = 100, message = "Title must be 5-100 characters"))]
    title: String,

    #[serde(deserialize_with = "trim_string")]
    #[validate(length(min = 10, message = "Content must be at least 10 characters"))]
    content: String,

    #[validate(custom(function = "validate_tags"))]
    tags: Vec<String>,
}

fn validate_tags(tags: &[String]) -> Result<(), validator::ValidationError> {
    if tags.len() > 5 {
        return Err(validator::ValidationError::new("too_many_tags"));
    }
    for tag in tags {
        if tag.len() > 20 {
            return Err(validator::ValidationError::new("tag_too_long"));
        }
    }
    Ok(())
}

// ============================================================================
// SECTION 8: CONTROLLER IMPLEMENTATION
// ============================================================================

#[controller("/api", pub struct ExamplesController;)]
impl ExamplesController {
    /// Example 1: Basic extraction (no validation)
    /// curl -X POST http://localhost:3000/api/users \
    ///   -H "Content-Type: application/json" \
    ///   -d '{"name":"John","email":"john@example.com"}'
    #[post("/users")]
    fn create_user(&self, Json(dto): Json<CreateUserDto>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "id": 1,
            "name": dto.name,
            "email": dto.email
        }))
    }

    /// Example 2: Path parameter with automatic parsing
    /// curl http://localhost:3000/api/users/123
    #[get("/users/{id}")]
    fn get_user(&self, Path(params): Path<UserParams>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "id": params.id,
            "name": "John Doe"
        }))
    }

    /// Example 3: Query parameters with optional values
    /// curl "http://localhost:3000/api/users?page=2&limit=20"
    #[get("/users")]
    fn list_users(&self, Query(query): Query<PaginationQuery>) -> ToniBody {
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(10);

        ToniBody::Json(serde_json::json!({
            "page": page,
            "limit": limit,
            "items": []
        }))
    }

    /// Example 4: Validation with Validated wrapper
    /// Valid request:
    /// curl -X POST http://localhost:3000/api/products \
    ///   -H "Content-Type: application/json" \
    ///   -d '{"name":"Widget","price":29.99,"contact_email":"sales@example.com"}'
    ///
    /// Invalid request (returns 400):
    /// curl -X POST http://localhost:3000/api/products \
    ///   -H "Content-Type: application/json" \
    ///   -d '{"name":"Wi","price":-10,"contact_email":"invalid"}'
    #[post("/products")]
    fn create_product(&self, Validated(Json(dto)): Validated<Json<CreateProductDto>>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "success": true,
            "product": {
                "name": dto.name,
                "price": dto.price,
                "contact_email": dto.contact_email
            }
        }))
    }

    /// Example 5: Automatic type parsing (Path<i32>)
    /// curl http://localhost:3000/api/products/42
    /// curl http://localhost:3000/api/products/abc  ← Returns 400 automatically
    #[get("/products/{id}")]
    fn get_product(&self, Path(params): Path<ProductIdParam>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "id": params.id,
            "name": "Product"
        }))
    }

    /// Example 6: Bool and int parsing in query
    /// curl "http://localhost:3000/api/search?active=true&min_price=100"
    #[get("/search")]
    fn search(&self, Query(query): Query<SearchQuery>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "active": query.active,
            "min_price": query.min_price,
            "results": []
        }))
    }

    /// Example 7: Query with defaults (serde default)
    /// curl "http://localhost:3000/api/items"  ← Uses defaults
    /// curl "http://localhost:3000/api/items?page=3&limit=50"
    #[get("/items")]
    fn list_items(&self, Query(query): Query<ItemsQueryWithDefaults>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "page": query.page,
            "limit": query.limit,
            "items": []
        }))
    }

    /// Example 8: Custom deserializer (trim strings)
    /// curl -X POST http://localhost:3000/api/comments \
    ///   -H "Content-Type: application/json" \
    ///   -d '{"text":"  Great post!  ","author":"  John  "}'
    /// Result: Trimmed strings + validation
    #[post("/comments")]
    fn create_comment(&self, Validated(Json(dto)): Validated<Json<CreateCommentDto>>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "success": true,
            "comment": {
                "text": dto.text,  // Trimmed
                "author": dto.author  // Trimmed
            }
        }))
    }

    /// Example 9: Complex custom validation (password match)
    /// curl -X POST http://localhost:3000/api/register \
    ///   -H "Content-Type: application/json" \
    ///   -d '{"email":"user@example.com","password":"SecurePass123","confirm_password":"SecurePass123"}'
    #[post("/register")]
    fn register(&self, Validated(Json(dto)): Validated<Json<RegisterDto>>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "success": true,
            "email": dto.email
        }))
    }

    /// Example 10: Full composition (trim + validate + custom rules)
    /// curl -X POST http://localhost:3000/api/posts \
    ///   -H "Content-Type: application/json" \
    ///   -d '{"title":"  My Post  ","content":"  This is content  ","tags":["rust","web"]}'
    #[post("/posts")]
    fn create_post(&self, Validated(Json(dto)): Validated<Json<CreatePostDto>>) -> ToniBody {
        ToniBody::Json(serde_json::json!({
            "success": true,
            "post": {
                "title": dto.title,
                "content": dto.content,
                "tags": dto.tags
            }
        }))
    }
}

// ============================================================================
// SECTION 9: WHY NO GLOBAL PIPES?
// ============================================================================

/// NestJS Has Global Pipes:
/// ```typescript
/// app.useGlobalPipes(new ValidationPipe({
///   whitelist: true,
///   forbidNonWhitelisted: true,
///   transform: true,
/// }));
/// ```
///
/// WHY TONI DOESN'T:
///
/// 1. **Type Safety**: In NestJS, validation happens at runtime. You can forget
///    to add @UsePipes() and data goes through unvalidated. In Toni, if you
///    don't use `Validated<T>`, the compiler knows - it's in the type.
///
/// 2. **Explicitness**: Rust philosophy = explicit over implicit. When you see
///    `Validated<Json<CreateUserDto>>`, you KNOW validation happens. In NestJS,
///    it's hidden in app.ts far from the handler.
///
/// 3. **Performance**: Global pipes run on EVERY request. Toni only validates
///    when you explicitly use `Validated<T>`. No wasted CPU cycles.
///
/// 4. **No Runtime Reflection**: NestJS needs global pipes because TypeScript
///    types disappear at runtime. Rust types are REAL - they exist at runtime
///    and compile time.
///
/// WHAT TO USE INSTEAD:
///
/// If you want cross-cutting concerns (logging, auth, rate limiting), use:
/// - **Middleware**: For request/response interception
/// - **Guards**: For authorization checks
/// - **Interceptors**: For response transformation
///
/// But validation? That's a TYPE-LEVEL concern. Use `Validated<T>`.

// ============================================================================
// SECTION 10: MODULE SETUP
// ============================================================================

#[module(controllers: [ExamplesController])]
impl ExamplesModule {}

#[tokio::main]
async fn main() {
    let mut app = toni::ToniFactory::create(ExamplesModule).await;

    app.use_http_adapter(toni_axum::AxumAdapter::new("127.0.0.1", 3000)).unwrap();

    println!("🚀 Toni Pipes Examples Server");
    println!("============================");
    println!();
    println!("📖 API Endpoints:");
    println!("  POST   /api/users          - Basic JSON extraction");
    println!("  GET    /api/users/:id      - Path parameter");
    println!("  GET    /api/users          - Query parameters");
    println!("  POST   /api/products       - Validated JSON");
    println!("  GET    /api/products/:id   - Parsed path param (int)");
    println!("  GET    /api/search         - Parsed query (bool, int)");
    println!("  GET    /api/items          - Query with defaults");
    println!("  POST   /api/comments       - Custom deserializer (trim)");
    println!("  POST   /api/register       - Complex validation");
    println!("  POST   /api/posts          - Full composition");
    println!();
    println!("🌐 Server running on http://localhost:3000");
    println!();
    println!("💡 TIP: Check the source code comments for NestJS comparisons!");

    app.start().await
}

// ============================================================================
// KEY TAKEAWAYS FOR NESTJS DEVELOPERS
// ============================================================================

// 1. NO MORE @UsePipes() EVERYWHERE
//    - NestJS: @Body(ValidationPipe) on every handler
//    - Toni: Just use Validated<Json<T>> when you need it
//
// 2. TYPES DO THE PARSING
//    - NestJS: @Param('id', ParseIntPipe) id: number
//    - Toni: Path<i32> - automatic
//
// 3. VALIDATION IS EXPLICIT
//    - NestJS: Global ValidationPipe (hidden)
//    - Toni: Validated<T> in signature (visible)
//
// 4. DEFAULTS ARE TYPE-LEVEL
//    - NestJS: @Query('page', new DefaultValuePipe(1))
//    - Toni: #[serde(default = "default_page")] or Option<T>.unwrap_or()
//
// 5. CUSTOM PIPES → CUSTOM DESERIALIZERS
//    - NestJS: implements PipeTransform
//    - Toni: #[serde(deserialize_with = "custom_fn")]
//
// 6. NO GLOBAL PIPES
//    - NestJS: app.useGlobalPipes() for convenience
//    - Toni: Explicit per-handler - compiler-enforced safety

// CHEAT SHEET:
//
// | NestJS Pattern                          | Toni Equivalent                    |
// |-----------------------------------------|------------------------------------|
// | @Body()                                 | Json<T>                            |
// | @Body(ValidationPipe)                   | Validated<Json<T>>                 |
// | @Param('id')                            | Path<ParamsStruct>                 |
// | @Param('id', ParseIntPipe)              | Path<i32> (automatic)              |
// | @Query('page', DefaultValuePipe(1))     | #[serde(default)] or Option<T>     |
// | @UsePipes(CustomPipe)                   | #[serde(deserialize_with)]         |
// | app.useGlobalPipes(ValidationPipe)      | Use Validated<T> explicitly        |
// | class-validator decorators              | validator::Validate derive         |
// | @IsEmail(), @MinLength()                | #[validate(email)], #[validate...] |
//
// This cheat sheet is your quick reference when migrating NestJS code to Toni!
