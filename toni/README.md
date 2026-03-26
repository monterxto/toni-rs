# Toni Framework

**Toni** is a Rust backend framework designed for building modular and scalable applications inspired by the Nest.js architecture. It provides a structured approach to organizing your code with controllers, services, and modules, while remaining decoupled from the HTTP server (Axum adapted and used by default).

---

## Features

- **Modular Architecture**: Organize your application into reusable modules.
- **HTTP Server Flexibility**: Use Axum or integrate your preferred server.
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

### Decoupled HTTP Server

Toni decouples your application from the HTTP server, and by default we use Axum. In the future we plan to integrate other HTTP adapters.

## Code Example

**`main.rs`**

```rust
use toni::{ToniFactory, AxumAdapter};

#[tokio::main]
async fn main() {
    let axum_adapter = AxumAdapter::new();

    let mut app = ToniFactory::create(AppModule::module_definition(), axum_adapter);
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
#[controller("/app", pub struct _AppController { app_service: _AppService })]
impl _AppController {
    #[post("")]
    fn create(&self, _req: HttpRequest) -> Body {
        Body::text(self.app_service.create())
    }

    #[get("")]
    fn find_all(&self, _req: HttpRequest) -> Body {
        Body::text(self.app_service.find_all())
    }
}
```

**`app/app.service.rs`** (Business Logic)

```rust
#[injectable(pub struct _AppService;)]
impl _AppService {
    pub fn create(&self) -> String {
        "Item created!".into()
    }

    pub fn find_all(&self) -> String {
        "All items!".into()
    }
}
```

---

## License

- **License**: MIT.

---
