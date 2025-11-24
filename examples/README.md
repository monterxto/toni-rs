# Toni Framework Examples

## Complete Pipes Guide for NestJS Developers

This examples crate contains a comprehensive, single-file demonstration of ALL pipe patterns in Toni compared to NestJS.

### 🎯 What This Example Covers

The `pipes_complete_guide.rs` example is the definitive guide showing:

1. **Basic Extractors** - `Json`, `Query`, `Path` equivalents to NestJS decorators
2. **Validation** - Why `Validated<T>` replaces `ValidationPipe`
3. **Type Parsing** - How `Path<i32>` replaces `ParseIntPipe`
4. **Optional Values** - Alternatives to `DefaultValuePipe`
5. **Custom Validation** - Custom deserializers vs custom pipes
6. **Complex Validation** - Schema-level validation
7. **Composition** - How extractors compose
8. **Global Pipes** - Why Toni doesn't need them
9. **Complete Examples** - 10 working endpoints with curl commands
10. **Cheat Sheet** - Quick NestJS → Toni reference

### 🚀 Running the Example

```bash
# From the repository root
cargo run --example pipes_complete_guide
```

The server will start on `http://localhost:3000` with 10 endpoints demonstrating different pipe patterns.

### 📖 Example Endpoints

All examples include:

- ✅ The NestJS equivalent (in comments)
- ✅ The Toni implementation
- ✅ Explanation of WHY it's different
- ✅ curl commands to test

#### 1. Basic JSON Extraction

```bash
curl -X POST http://localhost:3000/api/users \
  -H "Content-Type: application/json" \
  -d '{"name":"John","email":"john@example.com"}'
```

#### 2. Path Parameters

```bash
curl http://localhost:3000/api/users/123
```

#### 3. Query Parameters

```bash
curl "http://localhost:3000/api/users?page=2&limit=20"
```

#### 4. Validated Request (with Validator)

```bash
# Valid
curl -X POST http://localhost:3000/api/products \
  -H "Content-Type: application/json" \
  -d '{"name":"Widget","price":29.99,"contact_email":"sales@example.com"}'

# Invalid (returns 400)
curl -X POST http://localhost:3000/api/products \
  -H "Content-Type: application/json" \
  -d '{"name":"Wi","price":-10,"contact_email":"invalid"}'
```

#### 5. Automatic Type Parsing

```bash
# Valid integer
curl http://localhost:3000/api/products/42

# Invalid (returns 400 automatically)
curl http://localhost:3000/api/products/abc
```

#### 6. Boolean & Integer Query Parsing

```bash
curl "http://localhost:3000/api/search?active=true&min_price=100"
```

#### 7. Default Values

```bash
# Uses defaults (page=1, limit=10)
curl "http://localhost:3000/api/items"

# Custom values
curl "http://localhost:3000/api/items?page=3&limit=50"
```

#### 8. Custom Deserializer (Trim Strings)

```bash
curl -X POST http://localhost:3000/api/comments \
  -H "Content-Type: application/json" \
  -d '{"text":"  Great post!  ","author":"  John  "}'
```

#### 9. Complex Validation (Password Match)

```bash
curl -X POST http://localhost:3000/api/register \
  -H "Content-Type: application/json" \
  -d '{"email":"user@example.com","password":"SecurePass123","confirm_password":"SecurePass123"}'
```

#### 10. Full Composition (Trim + Validate + Custom Rules)

```bash
curl -X POST http://localhost:3000/api/posts \
  -H "Content-Type: application/json" \
  -d '{"title":"  My Post  ","content":"  This is content  ","tags":["rust","web"]}'
```

## 🔑 Key Differences: NestJS vs Toni

### 1. No Global Pipes

**NestJS:**

```typescript
// In main.ts - applies to ALL routes
app.useGlobalPipes(new ValidationPipe());
```

**Toni:**

```rust
// Explicit per handler
fn create(&self, Validated(Json(dto)): Validated<Json<CreateUserDto>>) -> ToniBody
```

**Why?** Rust's type system makes validation part of the signature, not runtime magic.

### 2. Types Do the Parsing

**NestJS:**

```typescript
@Get(':id')
findOne(@Param('id', ParseIntPipe) id: number) { }
```

**Toni:**

```rust
#[get("/:id")]
fn find_one(&self, Path(params): Path<i32>) -> ToniBody { }
```

**Why?** Serde deserializes based on type automatically.

### 3. Validation is Explicit

**NestJS:**

```typescript
// Validation hidden in global config
@Post()
create(@Body() dto: CreateUserDto) { }
```

**Toni:**

```rust
// Validation visible in signature
#[post("/")]
fn create(&self, Validated(Json(dto)): Validated<Json<CreateUserDto>>) -> ToniBody { }
```

**Why?** Explicitness over magic - you SEE validation requirements.

### 4. Defaults are Type-Level

**NestJS:**

```typescript
@Get()
findAll(@Query('page', new DefaultValuePipe(1)) page: number) { }
```

**Toni Option 1:**

```rust
#[get("/")]
fn find_all(&self, Query(query): Query<PaginationQuery>) -> ToniBody {
    let page = query.page.unwrap_or(1);
}
```

**Toni Option 2:**

```rust
#[derive(Deserialize)]
struct PaginationQuery {
    #[serde(default = "default_page")]
    page: u32,
}
fn default_page() -> u32 { 1 }
```

### 5. Custom Pipes → Custom Deserializers

**NestJS:**

```typescript
@Injectable()
export class TrimPipe implements PipeTransform {
  transform(value: any) {
    return typeof value === "string" ? value.trim() : value;
  }
}
```

**Toni:**

```rust
fn trim_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where D: Deserializer<'de> {
    let s = String::deserialize(deserializer)?;
    Ok(s.trim().to_string())
}

#[derive(Deserialize)]
struct CreateCommentDto {
    #[serde(deserialize_with = "trim_string")]
    text: String,
}
```

## 📚 Quick Reference Cheat Sheet

| NestJS Pattern                        | Toni Equivalent                    |
| ------------------------------------- | ---------------------------------- |
| `@Body()`                             | `Json<T>`                          |
| `@Body(ValidationPipe)`               | `Validated<Json<T>>`               |
| `@Param('id')`                        | `Path<ParamsStruct>`               |
| `@Param('id', ParseIntPipe)`          | `Path<i32>` (automatic)            |
| `@Query('page')`                      | `Query<QueryStruct>`               |
| `@Query('page', DefaultValuePipe(1))` | `#[serde(default)]` or `Option<T>` |
| `@UsePipes(CustomPipe)`               | `#[serde(deserialize_with)]`       |
| `app.useGlobalPipes(ValidationPipe)`  | Use `Validated<T>` explicitly      |
| `@IsEmail()`, `@MinLength()`          | `#[validate(email)]`, etc.         |
| `class-validator` decorators          | `validator::Validate` derive       |

## 🎓 Learning Path

1. **Read the source code** - `pipes_complete_guide.rs` has extensive comments comparing NestJS and Toni
2. **Run the example** - Test all 10 endpoints with the provided curl commands
3. **Modify examples** - Try changing validation rules, adding new fields
4. **Build your own** - Apply these patterns to your own API

## 💡 Pro Tips

### When to Use `Validated<T>`

```rust
// ✅ DO: Use Validated when you need validation
#[post("/users")]
fn create(&self, Validated(Json(dto)): Validated<Json<CreateUserDto>>) -> ToniBody

// ❌ DON'T: Use Validated unnecessarily
#[get("/users/:id")]
fn get(&self, Validated(Path(id)): Validated<Path<i32>>) -> ToniBody
// ^ Overkill - Path<i32> already validates it's an integer
```

### Composing Extractors

```rust
// This works! Validated wraps Json
Validated(Json(dto)): Validated<Json<CreateUserDto>>

// This also works! Json destructures directly
Json(dto): Json<CreateUserDto>

// This is the pattern for validation
Validated(Json(dto)): Validated<Json<ValidatedDto>>
```

### Error Handling

```rust
// Validation errors return 400 automatically
Validated(Json(dto)): Validated<Json<CreateUserDto>>
// If validation fails → 400 Bad Request with error details

// Type parsing errors also return 400
Path(id): Path<i32>
// If path is not an integer → 400 Bad Request
```

## 🤔 FAQ for NestJS Developers

### Q: Why can't I use global validation like NestJS?

**A:** You don't need it! In NestJS, global validation prevents you from forgetting to add `ValidationPipe`. In Toni, the compiler enforces it - if you use `Validated<T>`, validation happens. If you don't, it doesn't. Type safety replaces runtime magic.

### Q: How do I validate nested objects?

**A:** The `validator` crate supports nested validation:

```rust
#[derive(Deserialize, Validate)]
struct CreateUserDto {
    #[validate]  // ← Validates nested object
    address: Address,
}

#[derive(Deserialize, Validate)]
struct Address {
    #[validate(length(min = 2))]
    city: String,
}
```

### Q: What about transformation pipes like `ParseArrayPipe`?

**A:** Serde handles it automatically:

```rust
#[derive(Deserialize)]
struct QueryDto {
    tags: Vec<String>,  // ?tags=rust&tags=web → ["rust", "web"]
}
```

### Q: Can I validate query parameters?

**A:** Yes! Use `Validated<Query<T>>`:

```rust
#[derive(Deserialize, Validate)]
struct SearchQuery {
    #[validate(range(min = 1, max = 100))]
    limit: u32,
}

#[get("/search")]
fn search(&self, Validated(Query(q)): Validated<Query<SearchQuery>>) -> ToniBody
```

### Q: How do I handle multiple validation errors?

**A:** The `validator` crate returns all errors:

```rust
// When validation fails, you get ALL validation errors in the response
// Example:
// {
//   "name": ["Name must be at least 3 characters"],
//   "email": ["Invalid email format"],
//   "price": ["Price cannot be negative"]
// }
```

## 🔗 Related Documentation

- [Extractor System](../toni/src/extractors/README.md) - Deep dive into extractors
- [Validation Guide](../toni/src/extractors/validated.rs) - How `Validated<T>` works
- [Validator Crate](https://docs.rs/validator/) - Validation rules reference

## 🗂️ Other Examples

### Middleware Examples

The [middleware_examples.rs](middleware_examples.rs) file contains reference implementations for common middleware patterns:

- **LoggerMiddleware** - Request/response logging with timing
- **CorsMiddleware** - CORS headers and preflight handling
- **AuthMiddleware** - Bearer token authentication
- **TimeoutMiddleware** - Request timeout handling
- **CompressionMiddleware** - Response compression (placeholder)
- **RateLimitMiddleware** - In-memory rate limiting

Run with:

```bash
cargo run --example middleware_examples
```

Note: These are educational examples, not production-ready implementations.

## 📝 License

MIT - Same as Toni framework
