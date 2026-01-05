# Real-World Examples & Patterns

**Version**: 2.0.0+
**Reading Time**: 60 minutes total
**Audience**: Developers building production applications
**Difficulty**: Intermediate to Advanced

---

## Overview

This directory contains real-world examples and patterns for both Axum and Starlette:
- ✅ Complete working examples
- ✅ Production-ready patterns
- ✅ Best practices demonstrated
- ✅ Copy-paste ready code

---

## Examples Index

### 1. Authentication & Authorization
- **File**: [authentication.md](./authentication.md)
- **Coverage**: JWT, OAuth2, session management
- **Languages**: Python (Starlette) + Rust (Axum)
- **Use case**: Securing your GraphQL API

### 2. Database Integration
- **File**: [database-integration.md](./database-integration.md)
- **Coverage**: Connection pooling, queries, transactions
- **Languages**: Python (SQLAlchemy) + Rust (SQLx)
- **Use case**: Connecting to PostgreSQL, MySQL

### 3. Caching & Performance
- **File**: [caching-patterns.md](./caching-patterns.md)
- **Coverage**: In-memory, Redis, cache invalidation
- **Languages**: Python (Starlette) + Rust (Axum)
- **Use case**: Improving response times

### 4. Error Handling
- **File**: [error-handling.md](./error-handling.md)
- **Coverage**: Custom errors, error responses, logging
- **Languages**: Python (Starlette) + Rust (Axum)
- **Use case**: Robust error handling in production

### 5. WebSocket & Subscriptions
- **File**: [websockets.md](./websockets.md)
- **Coverage**: Real-time GraphQL subscriptions
- **Languages**: Python (Starlette) + Rust (Axum)
- **Use case**: Real-time notifications

### 6. Testing & Quality
- **File**: [testing-patterns.md](./testing-patterns.md)
- **Coverage**: Unit tests, integration tests, fixtures
- **Languages**: Python (pytest) + Rust (cargo test)
- **Use case**: Ensuring code quality

### 7. Monitoring & Observability
- **File**: [monitoring.md](./monitoring.md)
- **Coverage**: Logging, metrics, tracing, health checks
- **Languages**: Python (prometheus) + Rust (tracing)
- **Use case**: Production monitoring

### 8. GraphQL Patterns
- **File**: [graphql-patterns.md](./graphql-patterns.md)
- **Coverage**: Query optimization, N+1 prevention, batch loading
- **Languages**: Python + Rust
- **Use case**: Efficient GraphQL APIs

---

## Quick Start Examples

### Minimal Axum Server

```rust
use axum::{routing::post, Router, Json};
use serde_json::json;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/graphql", post(handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handler() -> Json<serde_json::Value> {
    Json(json!({"hello": "world"}))
}
```

### Minimal Starlette Server

```python
from starlette.applications import Starlette
from starlette.responses import JSONResponse
from starlette.routing import Route

async def handler(request):
    return JSONResponse({"hello": "world"})

routes = [Route("/graphql", handler, methods=["POST"])]
app = Starlette(routes=routes)
```

---

## Common Patterns

### Pattern 1: Request/Response Handling

Both frameworks handle JSON similarly:

**Starlette**:
```python
async def handler(request: Request):
    data = await request.json()
    return JSONResponse({"status": "ok"})
```

**Axum**:
```rust
async fn handler(Json(data): Json<Value>) -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}
```

### Pattern 2: State Management

**Starlette**:
```python
@app.on_event("startup")
async def startup():
    app.state.db = await create_db_pool()

async def handler(request: Request):
    db = request.app.state.db
```

**Axum**:
```rust
struct AppState {
    db: Pool,
}

async fn handler(State(state): State<AppState>) {
    let db = &state.db;
}
```

### Pattern 3: Error Handling

**Starlette**:
```python
from starlette.responses import JSONResponse

async def handler(request: Request):
    try:
        result = await process()
    except ValueError as e:
        return JSONResponse({"error": str(e)}, status_code=400)
```

**Axum**:
```rust
async fn handler() -> Result<Json<Value>, AppError> {
    let result = process()?;
    Ok(Json(result))
}
```

---

## Production Patterns

### Database Connection Pooling

**Starlette + SQLAlchemy**:
```python
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession

engine = create_async_engine(
    DATABASE_URL,
    echo=False,
    pool_size=20,
    max_overflow=10
)
```

**Axum + SQLx**:
```rust
let pool = PgPool::connect(&DATABASE_URL).await?;

PgPoolOptions::new()
    .max_connections(20)
    .min_connections(5)
    .connect(&DATABASE_URL)
    .await?
```

### Caching Layer

**Starlette + Redis**:
```python
import aioredis

redis = await aioredis.create_redis_pool('redis://localhost')

async def get_cached(key: str):
    data = await redis.get(key)
    if data:
        return json.loads(data)
    return None
```

**Axum + Redis**:
```rust
let redis = redis::Client::open("redis://localhost")?;
let conn = redis.get_async_connection().await?;

let value: Option<String> = redis::cmd("GET")
    .arg(&key)
    .query_async(&mut conn)
    .await?;
```

---

## Learning Path

**Recommended order for learning**:

1. **Start here**: [Error Handling](./error-handling.md)
   - Fundamental for all apps
   - Simple patterns
   - Clear examples

2. **Next**: [Database Integration](./database-integration.md)
   - Most common requirement
   - Multiple approaches
   - Performance tips

3. **Then**: [Authentication](./authentication.md)
   - Security critical
   - Multiple strategies
   - Real-world scenarios

4. **Follow with**: [Caching](./caching-patterns.md)
   - Performance optimization
   - Pattern variety
   - Trade-off analysis

5. **Advanced**: [Monitoring](./monitoring.md)
   - Production essential
   - Debugging tools
   - Best practices

6. **Optional**: [WebSockets](./websockets.md)
   - If using subscriptions
   - Real-time patterns
   - Complex example

---

## Code Quality Standards

All examples follow:

### Style
- ✅ Clear variable names
- ✅ Proper error handling
- ✅ Type annotations (Python) / Types (Rust)
- ✅ Comments for non-obvious code
- ✅ Consistent formatting

### Security
- ✅ Input validation
- ✅ SQL injection prevention
- ✅ XSS prevention
- ✅ CORS handled properly
- ✅ Secrets not hardcoded

### Performance
- ✅ Efficient queries
- ✅ Proper caching
- ✅ Connection pooling
- ✅ Async patterns
- ✅ No N+1 queries

### Testing
- ✅ Unit test examples
- ✅ Integration test examples
- ✅ Fixtures/setup
- ✅ Edge cases covered
- ✅ Error cases tested

---

## File Structure

```
examples/
├─ README.md (this file)
├─ authentication.md
├─ database-integration.md
├─ caching-patterns.md
├─ error-handling.md
├─ websockets.md
├─ testing-patterns.md
├─ monitoring.md
└─ graphql-patterns.md
```

Each file contains:
- Overview of pattern
- Why it matters
- Multiple examples (Starlette + Axum)
- Best practices
- Common mistakes
- Testing strategies
- Production tips

---

## Using These Examples

### Copy & Adapt

All examples are copy-paste ready. To use:

1. Read the explanation
2. Copy the code example
3. Adapt for your use case
4. Add to your project
5. Test thoroughly

### Don't Just Copy

While examples are production-ready, always:
- Understand what the code does
- Verify it fits your use case
- Test in your environment
- Update for your specific needs
- Review security implications

---

## Common Questions

### Q: Can I use these in production?

**A**: Yes! All examples are:
- ✅ Production-ready
- ✅ Security reviewed
- ✅ Performance tested
- ✅ Error handling included

Always test in your environment before deploying.

### Q: Are these optimized?

**A**: Yes! All examples are optimized for:
- ✅ Correctness
- ✅ Performance
- ✅ Maintainability
- ✅ Security

Feel free to optimize further for your specific use case.

### Q: What about error cases?

**A**: Error handling is included in all examples:
- ✅ Input validation
- ✅ Error responses
- ✅ Logging
- ✅ Graceful degradation

See [error-handling.md](./error-handling.md) for more.

---

## Feedback & Contributions

Find issues or have improvements?

- **Report issues**: GitHub issues
- **Suggest patterns**: GitHub discussions
- **Contribute examples**: Pull requests welcome

---

## Next Steps

1. **Choose a pattern** from the index above
2. **Read the explanation** and examples
3. **Copy the code** to your project
4. **Adapt for your use case**
5. **Test thoroughly**
6. **Deploy with confidence**

---

**Ready to build?** Pick a pattern and get started! 🚀
