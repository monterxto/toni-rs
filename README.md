<p align="center">A Rust framework for building efficient and scalable server-side applications.</p>
<p align="center">

## Description

Toni is a framework for building efficient and scalable server-side Rust applications. It was inspired by NestJS architecture, offering a clean architecture and a developer-friendly experience.

Toni is framework-agnostic and is built to be easily integrated with other HTTP servers.

## Features

- **Modular Architecture**: Organize your application into reusable modules.
- **HTTP Server Flexibility**: Choose Axum (`toni-axum`), Actix-web (`toni-actix`), or bring your own by implementing the `HttpAdapter` trait.
- **Dependency Injection**: Manage dependencies cleanly with module providers.
- **Macro-Driven Syntax**: Reduce boilerplate with intuitive procedural macros.

---

## Installation

### Prerequisites

- **[Rust & Cargo](https://www.rust-lang.org/tools/install)**: Ensure Rust is installed.
- **Toni CLI**: Install the CLI tool globally:
  ```bash
  cargo install toni-cli
  ```

---

## Quickstart: Build a CRUD App

Use the Toni CLI to create a new project:

```bash
toni new my_app
```

## Project Structure

```
src/
├── app/
│   ├── app.controller.rs
│   ├── app.module.rs
│   ├── app.service.rs
│   └── mod.rs
└── main.rs
```

## Run the Server

```bash
cargo run
```

Test your endpoints at `http://localhost:3000/app`.

---

## Key Concepts

### Project Structure

| File                    | Role                                      |
| ----------------------- | ----------------------------------------- |
| **`app.controller.rs`** | Defines routes and handles HTTP requests. |
| **`app.module.rs`**     | Configures dependencies and module setup. |
| **`app.service.rs`**    | Implements core business logic.           |

### HTTP Server Adapters

Toni is decoupled from HTTP servers. Choose your adapter:

- **toni-axum**: Axum + Tokio (I/O-bound workloads)
- **toni-actix**: Actix-web (CPU-bound workloads)
- **Bring your own**: Implement the `HttpAdapter` trait to integrate any HTTP server

## Code Example

**`main.rs`** (with Axum)

```rust
use toni::ToniFactory;
use toni_axum::AxumAdapter;

#[tokio::main]
async fn main() {
    let adapter = AxumAdapter::new();
    let mut app = ToniFactory::create(AppModule::module_definition(), adapter);
    app.listen(3000, "127.0.0.1").await;
}
```

**Or with Actix:**

```rust
use toni::ToniFactory;
use toni_actix::ActixAdapter;

#[actix_web::main]
async fn main() {
    let adapter = ActixAdapter::new();
    let mut app = ToniFactory::create(AppModule::module_definition(), adapter);
    app.listen(3000, "127.0.0.1").await;
}
```

**`app/app.module.rs`** (Root Module)

```rust
#[module(
    imports: [],
    controllers: [_AppController],
    providers: [_AppService],
    exports: []
)]
pub struct AppModule;
```

**`app/app.controller.rs`** (HTTP Routes)

```rust
#[controller(
    "/app",
    pub struct _AppController {
        app_service: _AppService
    }
)]
impl _AppController {
    #[post("")]
    fn create(&self) -> Body {
        Body::text(self.app_service.create())
    }

    #[get("")]
    fn find_all(&self) -> Body {
        Body::text(self.app_service.find_all())
    }
}
```

**`app/app.service.rs`** (Business Logic)

```rust
#[injectable(
    pub struct _AppService;
)]
impl _AppService {
    pub fn create(&self) -> String {
        "Item created!".into()
    }

    pub fn find_all(&self) -> String {
        "All items!".into()
    }
}
```

## License

Toni is [MIT licensed](LICENSE).
