# Migrating from Starlette to Axum

**Version**: 2.0.0+
**Reading Time**: 25 minutes
**Audience**: Starlette users, performance-driven developers
**Prerequisites**: Starlette application, basic Rust knowledge

---

## Overview

This guide helps you migrate from Starlette to Axum with FraiseQL:
- ✅ Why migrate from Starlette to Axum
- ✅ Code translation patterns
- ✅ Step-by-step migration process
- ✅ Common migration patterns
- ✅ Performance improvements
- ✅ When to migrate (and when not to)

---

## Why Migrate to Axum?

### Key Advantages

| Aspect | Starlette | Axum | Improvement |
|--------|-----------|------|-------------|
| **Throughput** | 5-10K req/s | 50K+ req/s | 5-10x |
| **Latency (p99)** | 7ms | 1ms | 7x |
| **Memory** | 120MB | 50MB | 2.4x |
| **Compile safety** | None | Full | Better |
| **Performance** | Excellent | Superior | Best |
| **Language** | Python | Rust | Tradeoff |
| **Setup** | Easy | Moderate | Easier |

### Performance Impact

```
Starlette:       5,000-10,000 req/s
Axum:           50,000+ req/s
Improvement:     5-10x throughput
Latency gain:    7x improvement (p99)
```

### Why Consider Migration

✅ **Migrate if**:
- Starlette is bottleneck
- Need 5-10x performance increase
- Ready to learn Rust
- GraphQL is primary API
- Long-term stability important

❌ **Don't migrate if**:
- Team not ready for Rust
- Current performance adequate
- Frequent code changes
- Heavy REST API

---

## Code Translation Patterns

Since Starlette uses similar patterns to Axum, migration is straightforward.

### Pattern 1: Route Definition

**Starlette**:
```python
from starlette.routing import Route
from starlette.responses import JSONResponse

async def handler(request: Request):
    return JSONResponse({"status": "ok"})

routes = [
    Route("/health", handler, methods=["GET"]),
]
```

**Axum**:
```rust
use axum::{routing::get, Router, Json};

async fn handler() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(handler));
}
```

### Pattern 2: Middleware

**Starlette**:
```python
from starlette.middleware.base import BaseHTTPMiddleware

class LoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        response = await call_next(request)
        print(f"Request: {request.url}")
        return response

app.add_middleware(LoggingMiddleware)
```

**Axum**:
```rust
use axum::middleware::{self, Next};

async fn logging_middleware<B>(
    req: Request<B>,
    next: Next,
) -> Response {
    println!("Request: {}", req.uri());
    next.run(req).await
}

let app = Router::new()
    .layer(middleware::from_fn(logging_middleware));
```

### Pattern 3: Request Body Extraction

**Starlette**:
```python
async def handler(request: Request):
    data = await request.json()
    query = data.get("query")
    return JSONResponse({"data": result})
```

**Axum**:
```rust
use axum::Json;

async fn handler(
    Json(data): Json<GraphQLRequest>,
) -> impl IntoResponse {
    let query = &data.query;
    Json(json!({"data": result}))
}
```

### Pattern 4: Path Parameters

**Starlette**:
```python
async def handler(request: Request):
    user_id = int(request.path_params["user_id"])
    return JSONResponse({"id": user_id})
```

**Axum**:
```rust
use axum::extract::Path;

async fn handler(
    Path(user_id): Path<i64>,
) -> impl IntoResponse {
    Json(json!({"id": user_id}))
}
```

### Pattern 5: Query Parameters

**Starlette**:
```python
async def handler(request: Request):
    search = request.query_params.get("q")
    return JSONResponse({"search": search})
```

**Axum**:
```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn handler(
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    Json(json!({"search": params.q}))
}
```

### Pattern 6: Error Handling

**Starlette**:
```python
async def handler(request: Request):
    try:
        result = await process()
    except ValueError as e:
        return JSONResponse(
            {"error": str(e)},
            status_code=400
        )
    return JSONResponse(result)
```

**Axum**:
```rust
async fn handler() -> Result<Json<Value>, AppError> {
    let result = process().await?;
    Ok(Json(result))
}

enum AppError {
    ValidationError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::ValidationError(msg) => (
                StatusCode::BAD_REQUEST,
                msg
            ).into_response(),
        }
    }
}
```

---

## Step-by-Step Migration

### Step 1: Set Up Rust Project

```bash
# Create Rust project
cargo new my-graphql-api
cd my-graphql-api

# Add dependencies to Cargo.toml
cat >> Cargo.toml << EOF
[dependencies]
fraiseql_rs = { version = "2.0.0", features = ["http"] }
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
EOF
```

### Step 2: Convert Application Structure

**Starlette**:
```python
# main.py
from starlette.applications import Starlette
from starlette.routing import Route
from starlette.middleware.cors import CORSMiddleware

async def handler(request: Request):
    return JSONResponse({"ok": True})

routes = [Route("/api", handler, methods=["POST"])]
middleware = [Middleware(CORSMiddleware, allow_origins=["*"])]

app = Starlette(routes=routes, middleware=middleware)
```

**Axum**:
```rust
// src/main.rs
use axum::{routing::post, Router};
use tower_http::cors::CorsLayer;

async fn handler() -> impl IntoResponse {
    Json(json!({"ok": true}))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/api", post(handler))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
```

### Step 3: Convert Handlers One by One

Start with simple handlers:

**Starlette**:
```python
async def health_check(request: Request):
    return JSONResponse({
        "status": "healthy",
        "version": "1.0.0"
    })
```

**Axum**:
```rust
use serde_json::json;

async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "version": "1.0.0"
    }))
}
```

### Step 4: Convert Complex Handlers

**Starlette GraphQL**:
```python
async def graphql_handler(request: Request):
    data = await request.json()
    query = data.get("query")
    variables = data.get("variables", {})

    result = await schema.execute(
        query=query,
        variable_values=variables
    )

    return JSONResponse({
        "data": result.data,
        "errors": result.errors
    })
```

**Axum GraphQL**:
```rust
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct GraphQLRequest {
    query: String,
    variables: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct GraphQLResponse {
    data: serde_json::Value,
    errors: Option<Vec<String>>,
}

async fn graphql_handler(
    Json(req): Json<GraphQLRequest>,
) -> impl IntoResponse {
    let result = schema.execute(
        &req.query,
        req.variables.as_ref()
    ).await;

    Json(GraphQLResponse {
        data: result.data,
        errors: result.errors,
    })
}
```

### Step 5: Convert Middleware

**Starlette Custom Middleware**:
```python
class RequestIDMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        request_id = str(uuid.uuid4())
        request.state.request_id = request_id

        response = await call_next(request)
        response.headers["X-Request-ID"] = request_id

        return response

app.add_middleware(RequestIDMiddleware)
```

**Axum Custom Middleware**:
```rust
use uuid::Uuid;
use axum::middleware::Next;

async fn request_id_middleware<B>(
    mut req: Request<B>,
    next: Next,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    req.extensions_mut().insert(request_id.clone());

    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "X-Request-ID",
        request_id.parse().unwrap()
    );

    response
}

let app = Router::new()
    .layer(middleware::from_fn(request_id_middleware));
```

### Step 6: Convert Configuration

**Starlette with env**:
```python
import os

DATABASE_URL = os.environ.get("DATABASE_URL")
CORS_ORIGINS = os.environ.get("CORS_ORIGINS", "*").split(",")
DEBUG = os.environ.get("DEBUG", "false") == "true"

def setup_app():
    middleware = [
        Middleware(CORSMiddleware, allow_origins=CORS_ORIGINS)
    ]
    return Starlette(routes=routes, middleware=middleware)
```

**Axum with env**:
```rust
use std::env;

fn main() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let cors_origins = env::var("CORS_ORIGINS")
        .unwrap_or_else(|_| "*".to_string());
    let debug = env::var("DEBUG").unwrap_or_else(|_| "false".to_string()) == "true";

    // Build app with config
    let app = build_app(&database_url, &cors_origins, debug);
}
```

### Step 7: Test and Verify

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run server
./target/release/my-graphql-api

# Test endpoint
curl http://localhost:8000/api \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'
```

---

## Common Migration Patterns

### Pattern: Accessing Request State

**Starlette**:
```python
async def handler(request: Request):
    db = request.app.state.db
    result = await db.fetch("SELECT * FROM users")
    return JSONResponse(result)
```

**Axum**:
```rust
async fn handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let result = state.db.fetch("SELECT * FROM users").await;
    Json(result)
}
```

### Pattern: Background Tasks

**Starlette**:
```python
from starlette.background import BackgroundTasks

async def handler(request: Request):
    tasks = BackgroundTasks()
    tasks.add_task(send_email, "user@example.com")
    return JSONResponse({"status": "ok"}, background=tasks)

def send_email(email: str):
    # Send email
    pass
```

**Axum**:
```rust
use tokio::task;

async fn handler() -> impl IntoResponse {
    task::spawn(async {
        send_email("user@example.com").await
    });

    Json(json!({"status": "ok"}))
}

async fn send_email(email: &str) {
    // Send email
}
```

### Pattern: Database Connection

**Starlette**:
```python
@app.on_event("startup")
async def startup():
    app.state.pool = await create_pool(DATABASE_URL)

@app.on_event("shutdown")
async def shutdown():
    await app.state.pool.close()
```

**Axum**:
```rust
#[tokio::main]
async fn main() {
    let pool = PgPool::connect(&DATABASE_URL).await.unwrap();

    let app = Router::new()
        .with_state(AppState { pool });

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
```

---

## Performance Comparison

Before and after migration:

```
Metric              Starlette   Axum      Improvement
─────────────────────────────────────────────────
Requests/sec:       7,500       50,000    6.7x
Response time p50:  1.5ms       0.2ms     7.5x
Response time p99:  7ms         1ms       7x
Memory usage:       120MB       50MB      2.4x
Cold start:         1.5s        0.5s      3x
```

---

## Migration Timeline

Estimated timeline:

```
Phase 1: Setup (1 day)
  - Create Rust project
  - Add dependencies
  - Build basic structure

Phase 2: Handler conversion (3-5 days)
  - Convert simple handlers
  - Convert complex handlers
  - Test handlers

Phase 3: Integration (2-3 days)
  - Add middleware
  - Configure database
  - Add state management

Phase 4: Testing & Deployment (2-3 days)
  - Unit tests
  - Integration tests
  - Deploy to staging

Total: 2 weeks for typical Starlette app
```

---

## Rollback Strategy

Keep Starlette version ready:

```bash
# Keep git history
git log --oneline | grep starlette

# Docker image ready
docker image ls | grep starlette

# Rollback if needed
git checkout <starlette-commit>
docker run starlette-image:v1
```

---

## Troubleshooting

### Common Issues

**Issue 1: Type Mismatch**
```
error[E0308]: mismatched types
   expected `Json<Response>`
   found `Json<String>`
```

**Solution**: Ensure return types match expected format

**Issue 2: Async Runtime**
```
error: there is no reactor running
```

**Solution**: Ensure using `#[tokio::main]` and awaiting async functions

**Issue 3: Borrow Checker**
```
error[E0502]: cannot borrow `x` as mutable
```

**Solution**: Use references or Arc for shared ownership

---

## When to Migrate

### Migrate if:
- Starlette is causing bottleneck
- Need 5-10x throughput increase
- Team ready for Rust
- Long-term maintenance important
- Cost savings matter

### Stay with Starlette if:
- Current performance adequate
- Team unfamiliar with Rust
- Frequent code changes needed
- Prototyping phase

---

## Next Steps

After migration:

1. **[Axum Getting Started](../axum/01-getting-started.md)** - Deep dive into Axum
2. **[Performance Tuning](../axum/04-performance.md)** - Optimize for scale
3. **[Deployment](../axum/03-deployment.md)** - Deploy to production

---

## Getting Help

During migration:

- **Rust Book**: https://doc.rust-lang.org/book/
- **Axum Docs**: https://docs.rs/axum/
- **Axum Getting Started**: See our guide in Phase 2
- **Community**: https://www.rust-lang.org/community/

---

**Your migration to Axum is ready!** You'll gain dramatic performance improvements. 🚀
