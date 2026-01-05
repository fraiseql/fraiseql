# Migrating from FastAPI to Axum

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: FastAPI users considering Rust, performance-critical applications
**Prerequisites**: FastAPI application, basic Rust knowledge

---

## Overview

This guide helps you migrate from FastAPI to Axum with FraiseQL:
- ✅ Why migrate from FastAPI to Axum
- ✅ Architecture differences
- ✅ Rust learning requirements
- ✅ Step-by-step migration process
- ✅ Performance expectations
- ✅ When to migrate (and when not to)

---

## Why Migrate to Axum?

### Key Advantages

| Aspect | FastAPI | Axum | Winner |
|--------|---------|------|--------|
| **Performance** | 5-8K req/s | 50K+ req/s | Axum |
| **Throughput** | Good | Excellent | Axum |
| **Memory** | 150MB | 50MB | Axum |
| **Concurrency** | Good | Excellent | Axum |
| **Latency (p99)** | 10ms | 1ms | Axum |
| **Language** | Python | Rust | FastAPI |
| **Setup** | Easy | Moderate | FastAPI |
| **Team training** | Minimal | Required | FastAPI |

### Performance Impact

```
FastAPI (optimized):  5,000-8,000 req/s
Axum:                50,000+ req/s
Improvement:         6-10x throughput increase
```

### When to Migrate

✅ **Migrate if**:
- Performance is critical (finance, gaming, real-time)
- Need sub-millisecond latency
- High-frequency trading or similar workload
- Team can learn Rust
- GraphQL is primary API

❌ **Don't migrate if**:
- Team has no Rust expertise
- Prototyping or MVP phase
- Current performance adequate
- Frequent API changes
- REST API heavily used

---

## Architecture Comparison

### FastAPI Structure

```
FastAPI App (Python)
├─ Request parsing (automatic)
├─ Validation (Pydantic)
├─ Business logic
├─ Database query
└─ Response serialization
```

### Axum Structure

```
Axum App (Rust)
├─ Request extraction (type-safe)
├─ Validation (compile-time)
├─ Business logic
├─ Database query
└─ Response serialization (SIMD-optimized)
```

### Key Differences

| Feature | FastAPI | Axum |
|---------|---------|------|
| **Type safety** | Runtime | Compile-time |
| **Error handling** | Try/except | Result types |
| **Async runtime** | AsyncIO | Tokio |
| **Validation** | Pydantic models | Serde + custom |
| **Middleware** | Decorator | Layer trait |
| **Performance** | Good | Excellent |
| **Compile safety** | None | Full |

---

## Rust Learning Requirements

Before migrating, you need Rust knowledge:

### Essential Concepts

1. **Ownership & Borrowing** (10 hours)
   - Memory management
   - References
   - Lifetimes

2. **Type System** (5 hours)
   - Enums
   - Generics
   - Traits

3. **Async/Await** (5 hours)
   - Futures
   - Tokio runtime
   - Stream handling

4. **Error Handling** (3 hours)
   - Result type
   - Error propagation
   - ? operator

**Total Learning**: 20-30 hours for basics, 40-60 hours for proficiency

### Learning Resources

- **The Rust Book**: https://doc.rust-lang.org/book/
- **Async Rust**: https://rust-lang.github.io/async-book/
- **Axum Documentation**: https://docs.rs/axum/
- **Time**: 1-2 weeks for learning, 2-3 weeks for initial implementation

---

## Step-by-Step Migration

### Step 1: Set Up Rust Project

```bash
# Create Rust project
cargo new my-graphql-api
cd my-graphql-api

# Add dependencies
cat >> Cargo.toml << EOF
[dependencies]
fraiseql_rs = { version = "2.0.0", features = ["http"] }
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio"] }
EOF
```

### Step 2: Analyze Python Code

Audit your FastAPI application:

```python
# Identify what to convert:
# 1. Route handlers (async functions)
# 2. Request/response types
# 3. Database queries
# 4. Business logic
# 5. Middleware
# 6. Error handling
```

### Step 3: Convert Route Handlers

**FastAPI**:
```python
from fastapi import FastAPI
from pydantic import BaseModel

class User(BaseModel):
    id: int
    name: str

app = FastAPI()

@app.post("/graphql")
async def graphql(request: Request):
    data = await request.json()
    return {"data": result}
```

**Axum**:
```rust
use axum::{routing::post, Router, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User {
    id: i64,
    name: String,
}

async fn graphql_handler(
    Json(request): Json<GraphQLRequest>,
) -> impl IntoResponse {
    let result = execute_query(&request).await;
    Json(result)
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/graphql", post(graphql_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Step 4: Convert Pydantic Models

**FastAPI (Pydantic)**:
```python
from pydantic import BaseModel

class User(BaseModel):
    id: int
    name: str
    email: str
```

**Axum (Serde)**:
```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User {
    id: i64,
    name: String,
    email: String,
}

// With validation
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct User {
    #[serde(default)]
    id: i64,
    name: String,
    email: String,
}
```

### Step 5: Convert Database Code

**FastAPI (SQLAlchemy)**:
```python
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

engine = create_engine("postgresql://...")

async with engine.connect() as conn:
    users = await conn.execute(select(User))
```

**Axum (SQLx)**:
```rust
use sqlx::{PgPool, Row};

#[tokio::main]
async fn main() {
    let pool = PgPool::connect("postgresql://...").await.unwrap();

    let users: Vec<User> = sqlx::query_as::<_, User>(
        "SELECT * FROM users"
    )
    .fetch_all(&pool)
    .await
    .unwrap();
}
```

### Step 6: Convert Error Handling

**FastAPI**:
```python
from fastapi import HTTPException

@app.get("/users/{user_id}")
async def get_user(user_id: int):
    if user_id < 0:
        raise HTTPException(status_code=400, detail="Invalid ID")
    return {"id": user_id}
```

**Axum**:
```rust
use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
};

async fn get_user(
    Path(user_id): Path<i64>,
) -> Result<Json<User>, AppError> {
    if user_id < 0 {
        return Err(AppError::InvalidId);
    }

    let user = fetch_user(user_id).await?;
    Ok(Json(user))
}

enum AppError {
    InvalidId,
    NotFound,
    DatabaseError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::InvalidId => (
                StatusCode::BAD_REQUEST,
                "Invalid ID"
            ).into_response(),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                "User not found"
            ).into_response(),
            AppError::DatabaseError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error"
            ).into_response(),
        }
    }
}
```

### Step 7: Convert Middleware

**FastAPI**:
```python
@app.middleware("http")
async def add_request_id(request: Request, call_next):
    request.state.request_id = uuid.uuid4()
    response = await call_next(request)
    response.headers["X-Request-ID"] = request.state.request_id
    return response
```

**Axum**:
```rust
use axum::middleware::{self, Next};
use axum::http::Request;

async fn add_request_id<B>(
    mut req: Request<B>,
    next: Next,
) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    req.extensions_mut().insert(request_id.clone());

    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "X-Request-ID",
        request_id.parse().unwrap()
    );
    response
}

let app = Router::new()
    .layer(middleware::from_fn(add_request_id));
```

### Step 8: Test and Verify

```bash
# Build project
cargo build --release

# Run tests
cargo test

# Run server
./target/release/my-graphql-api

# Test endpoint
curl http://localhost:8000/graphql \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'
```

---

## Common Migration Challenges

### Challenge 1: Rust Learning Curve

**Problem**: Team isn't familiar with Rust

**Solution**: Gradual learning and training

```
Week 1: Learn Rust basics
Week 2: Learn Axum and async
Week 3: Start simple Axum service
Week 4: Migrate FastAPI service
```

### Challenge 2: Compilation Errors

**Problem**: Rust compiler is strict

**Solution**: Read error messages carefully

```
error[E0382]: value used after being moved
    |
  | let x = value;
  | --- value moved here
  | let y = value;
  |     ^^^^^ value used after move

// Fix: Use reference or clone
let x = value.clone();
let y = value;
```

### Challenge 3: Type System Complexity

**Problem**: Type inference can be confusing

```rust
// ❌ Compile error - type ambiguous
let vec = vec![1, 2, 3];

// ✅ Fix - specify type explicitly
let vec: Vec<i32> = vec![1, 2, 3];
```

### Challenge 4: Async Complexity

**Problem**: Async patterns different from Python

**FastAPI**:
```python
async def handler():
    result = await some_async_function()
    return result
```

**Axum**:
```rust
async fn handler() -> impl IntoResponse {
    let result = some_async_function().await;
    Json(result)
}
```

### Challenge 5: Dependency Management

**Problem**: Cargo dependencies can conflict

```bash
# Check dependencies
cargo tree

# Update dependencies safely
cargo update --aggressive

# Check for security issues
cargo audit
```

---

## Performance Expectations

After migration, expect:

```
Metric              FastAPI    Axum      Improvement
─────────────────────────────────────────────────
Requests/sec:       5,000      50,000    10x
Response time p50:  2ms        0.2ms     10x
Response time p99:  10ms       1ms       10x
Memory per request: 1MB        0.1MB     10x
Startup time:       2.5s       0.5s      5x
```

### Cost Analysis

**Before migration** (FastAPI):
- 10 instances handling 50K req/s
- $2,000/month infrastructure

**After migration** (Axum):
- 1-2 instances handling 50K req/s
- $200/month infrastructure
- **90% cost reduction**

---

## Rollback Plan

If migration fails:

```bash
# Keep FastAPI version in git
git log --oneline | grep fastapi

# Keep infrastructure as-is
docker ps | grep fastapi-api

# Rollback if needed
git checkout <commit-hash>
docker run fastapi-image:v1
```

---

## When to Migrate vs. Stay

### Stay with FastAPI if:
- Team has no Rust experience
- Current performance adequate
- Prototyping or MVP phase
- Frequent API changes
- REST API heavily used
- Don't want learning curve

### Migrate to Axum if:
- Performance is critical
- Need 5-10x throughput
- Team willing to learn Rust
- Long-term stability important
- GraphQL is primary API
- Operating costs matter

---

## Migration Timeline

Typical timeline for teams:

```
Week 1-2:   Rust learning
Week 3:     Axum basics
Week 4-5:   Route conversion
Week 6:     Database integration
Week 7:     Middleware and error handling
Week 8:     Testing and optimization
```

**Total**: 2 months from decision to production

---

## Next Steps

After migration:

1. **[Performance Tuning](../axum/04-performance.md)** - Optimize Axum
2. **[Axum Configuration](../axum/02-configuration.md)** - Configure for production
3. **[Axum Deployment](../axum/03-deployment.md)** - Deploy to production

---

## Getting Help

During migration:

- **Rust Book**: https://doc.rust-lang.org/book/
- **Axum Docs**: https://docs.rs/axum/
- **Rust Community**: https://www.rust-lang.org/community/
- **Stack Overflow**: Tag: `rust` or `axum`
- **FraiseQL Docs**: Main documentation

---

**Your migration to Axum is planned!** You'll gain dramatic performance improvements. 🚀
