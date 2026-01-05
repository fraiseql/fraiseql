# Troubleshooting: Axum

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: Backend developers, DevOps engineers
**Prerequisites**: Any Axum guide ([Getting Started](./01-getting-started.md), [Configuration](./02-configuration.md), [Deployment](./03-deployment.md), [Performance](./04-performance.md))

---

## Overview

This guide helps you diagnose and fix common Axum issues:
- ✅ Startup and compilation errors
- ✅ Runtime errors and panics
- ✅ Performance degradation
- ✅ Connection and database issues
- ✅ WebSocket and streaming problems
- ✅ Memory leaks and resource exhaustion
- ✅ Security and authentication errors
- ✅ Getting help and debugging strategies

---

## Startup & Compilation Errors

### Error: "error: expected identifier, found 'async'"

**Cause**: Missing `#[tokio::main]` macro

**Before**:
```rust
async fn main() {
    // ...
}
```

**After**:
```rust
#[tokio::main]
async fn main() {
    // ...
}
```

**Why**: Axum requires a Tokio runtime. The macro sets it up for you.

---

### Error: "cannot find function 'route' in this scope"

**Cause**: Missing imports

**Fix**:
```rust
use axum::{
    routing::post,           // ← Add this
    Router,
};
```

**Quick check**: Are all routes defined using the correct path?
```rust
// ✅ Correct
.route("/graphql", post(handler))

// ❌ Incorrect
.route("/graphql", handler)  // Missing post()
```

---

### Error: "future is not Send"

**Cause**: Async function body contains non-Send types (e.g., `Rc<T>`)

**Example problem**:
```rust
async fn handler() -> impl IntoResponse {
    let rc = std::rc::Rc::new(5);  // ❌ Not Send!
    // ...
}
```

**Fix**: Use Send-safe types
```rust
async fn handler() -> impl IntoResponse {
    let arc = std::sync::Arc::new(5);  // ✅ Send-safe
    // ...
}
```

**For shared state**:
```rust
// ❌ Wrong
type SharedState = std::rc::Rc<State>;

// ✅ Correct
type SharedState = std::sync::Arc<State>;
```

---

### Error: "expected 'async' block, found 'fn'"

**Cause**: Handler should be async

**Fix**:
```rust
// ❌ Wrong
fn graphql_handler() -> impl IntoResponse {
    // ...
}

// ✅ Correct
async fn graphql_handler() -> impl IntoResponse {
    // ...
}
```

---

## Runtime Errors

### Error: "listen tcp bind: address already in use"

**Cause**: Port already in use

**Diagnosis**:
```bash
# Find process using port 8000
lsof -i :8000

# Or on Windows
netstat -ano | findstr :8000
```

**Solution 1: Kill existing process**:
```bash
# Linux/macOS
kill -9 <PID>

# Or wait a few seconds for socket to fully close
```

**Solution 2: Use different port**:
```rust
let addr = SocketAddr::from(([127, 0, 0, 1], 8001));  // Changed port
```

**Solution 3: Reuse address**:
```rust
use std::net::SocketAddr;

let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;

// Enable SO_REUSEADDR (Linux/macOS)
// Note: Axum handles this automatically
```

---

### Error: "Connection refused" when accessing localhost:8000

**Cause**: Server not actually listening, or listening on different host

**Diagnosis**:
```bash
# Check if server is running
ps aux | grep my-graphql-api

# Check which ports are listening
netstat -tlnp | grep 8000

# Try connecting
curl -v http://localhost:8000/health
```

**Typical causes**:
1. Server crashed or failed to start
2. Listening on `127.0.0.1` instead of `0.0.0.0`
3. Wrong port configured

**Fix**:
```rust
// ✅ Listen on all interfaces
let addr = SocketAddr::from(([0, 0, 0, 0], 8000));

// Check in logs:
println!("Server listening on {}", addr);
```

---

### Error: "there is no reactor running"

**Cause**: Trying to use async code outside of async context

**Example**:
```rust
#[tokio::main]
async fn main() {
    // ✅ This works
    let result = async_function().await;

    // ❌ This fails
    std::thread::spawn(|| {
        async_function().await  // No reactor!
    });
}
```

**Fix**:
```rust
// Option 1: Use tokio::spawn
tokio::spawn(async {
    async_function().await  // ✅ Has reactor
});

// Option 2: Create runtime inside thread
std::thread::spawn(|| {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        async_function().await  // ✅ Has reactor
    });
});
```

---

### Error: "panic: called 'Result::unwrap()' on an 'Err' value"

**Cause**: Unhandled error in code

**Quick fix**: Use `?` operator instead of `unwrap()`
```rust
// ❌ Panics on error
let value = some_fallible_operation().unwrap();

// ✅ Returns error gracefully
let value = some_fallible_operation()?;
```

**Add context**: Use `context()` for better error messages
```rust
use anyhow::Context;

let value = some_fallible_operation()
    .context("Failed to parse configuration")?;
    // Error includes: "Failed to parse configuration: actual error details"
```

---

## Performance Issues

### Slow Queries (> 100ms per request)

**Diagnosis**:
```bash
# Enable query logging
RUST_LOG=debug cargo run

# Look for queries taking > 100ms
# Watch for N+1 patterns (many small queries)
```

**Common causes**:

1. **Database connection exhaustion**
```rust
// Check pool stats
println!("Pool size: {}, Idle: {}", pool.size(), pool.num_idle());

// Fix: Increase pool size
.max_connections(50)
```

2. **Inefficient GraphQL queries**
```graphql
# ❌ Fetches all user fields
query {
  users {
    id
    name
    email
    posts {
      id
      title
      comments {
        id
        text
      }
    }
  }
}

# ✅ Only fetch needed fields
query {
  users {
    id
    name
  }
}
```

3. **N+1 Query Problem**
```rust
// ❌ Slow: 1 query for posts + N queries for authors
let posts = fetch_all_posts().await;
for post in posts {
    post.author = fetch_author(post.author_id).await;
}

// ✅ Fast: 2 queries total
let posts = fetch_all_posts().await;
let author_ids: Vec<_> = posts.iter().map(|p| p.author_id).collect();
let authors = fetch_authors_batch(&author_ids).await;
```

**Fix**: Use batch loading
```rust
use dataloader::Loader;

// Define batch loader
struct AuthorLoader;

#[async_trait]
impl dataloader::BatchFn<UserId, Author> for AuthorLoader {
    async fn load(&self, ids: &[UserId]) -> HashMap<UserId, Author> {
        fetch_authors_batch(ids).await
    }
}

// Use in resolver
let author = author_loader.load(user_id).await;
```

---

### High CPU Usage

**Diagnosis**:
```bash
# Generate flamegraph
cargo flamegraph

# Identify hot spots (functions using most CPU)
```

**Common causes**:

1. **Inefficient algorithms**
```rust
// ❌ O(n²) loop
for item in &items {
    for other in &items {
        process(item, other);
    }
}

// ✅ O(n) with HashMap
let mut map = HashMap::new();
for item in &items {
    map.insert(item.id, item);
}
```

2. **Lock contention**
```rust
// ❌ RwLock can cause contention
let shared = Arc::new(RwLock::new(data));

// ✅ DashMap is lock-free
let shared = Arc::new(DashMap::new());
```

3. **Excessive JSON serialization**
```rust
// ✅ Cache serialized results
let response = cache.get(&query).await
    .or_else(|| {
        let result = execute_query(&query).await;
        let serialized = serde_json::to_string(&result)?;
        cache.set(&query, &serialized);
        serialized
    });
```

---

### Memory Leak or Growing Memory

**Diagnosis**:
```bash
# Monitor memory over time
watch -n 1 'ps aux | grep my-graphql-api'

# Or use top
top -p <PID>

# Check for unbounded growth over hours
```

**Common causes**:

1. **Unbounded cache**
```rust
// ❌ Cache grows forever
let cache = Arc::new(DashMap::new());

// ✅ Use bounded cache with TTL
let cache: Cache<String, String> = Cache::builder()
    .max_capacity(10_000)  // Limit size
    .build();
```

2. **Circular Arc references**
```rust
// ❌ Creates cycle: A → B → A
let a = Arc::new(RefCell::new(None));
let b = Arc::new(RefCell::new(Some(a.clone())));
*a.borrow_mut() = Some(b.clone());  // Cycle!

// ✅ Use Weak for back-references
let a = Arc::new(RefCell::new(None));
let b = Arc::new(RefCell::new(Some(Arc::downgrade(&a))));
```

3. **Leaked database connections**
```rust
// ❌ Connection not released
let conn = pool.acquire().await?;
drop(conn);  // Explicitly release

// ✅ Better: use connection within scope
{
    let mut tx = pool.begin().await?;
    // Use tx
    tx.commit().await?;  // Auto-released
}
```

---

## Database Connection Issues

### Error: "Cannot connect to database"

**Diagnosis**:
```bash
# Test connection directly
psql $DATABASE_URL

# Check if PostgreSQL is running
pg_isready -h localhost -p 5432
```

**Common causes**:

1. **Wrong DATABASE_URL**
```bash
# ❌ Wrong
DATABASE_URL=postgres://localhost/db

# ✅ Correct format
DATABASE_URL=postgresql://user:password@localhost:5432/dbname
```

2. **PostgreSQL not running**
```bash
# Start PostgreSQL
brew services start postgresql  # macOS
sudo systemctl start postgresql # Linux
```

3. **Firewall blocking connection**
```bash
# Check if port is open
telnet localhost 5432

# Or on Linux
nc -zv localhost 5432
```

**Fix**:
```rust
// Add retry logic
let mut retries = 0;
loop {
    match PgPool::connect(&database_url).await {
        Ok(pool) => break pool,
        Err(e) if retries < 5 => {
            eprintln!("Connection failed, retrying... {}", e);
            tokio::time::sleep(Duration::from_secs(2)).await;
            retries += 1;
        }
        Err(e) => panic!("Cannot connect to database: {}", e),
    }
}
```

---

### Error: "Connection pool timed out"

**Cause**: All connections busy, new request can't get one

**Diagnosis**:
```rust
// Log pool stats
let idle = pool.num_idle();
let size = pool.size();
let busy = size - idle;

log::info!("Pool: idle={}, busy={}, total={}", idle, busy, size);
```

**Typical sequence**:
```
Time 1: All 20 connections in use, request arrives
Time 2: New request waits for connection (timeout 30s)
Time 3: No connection available after 30s → Timeout error
```

**Fix**:

1. **Increase pool size**
```rust
.max_connections(50)  // Increase from default 20
```

2. **Optimize query time**
```rust
// Make database queries faster
// - Add indexes
// - Optimize GraphQL queries
// - Batch requests
```

3. **Increase timeout**
```rust
.acquire_timeout(Duration::from_secs(60))  // Was 30s
```

---

### Error: "too many connections"

**Cause**: PostgreSQL max_connections limit exceeded

**Check limit**:
```sql
SHOW max_connections;  -- Default: 100
```

**Fix**:
1. **Reduce pool size**
```rust
.max_connections(20)  // Was 50
```

2. **Use PgBouncer** (connection pooler)
```
# PgBouncer forwards connections to PostgreSQL
Client → PgBouncer (1000 conn) → PostgreSQL (100 conn)
```

3. **Increase PostgreSQL limit**
```sql
ALTER SYSTEM SET max_connections = 200;
SELECT pg_reload_conf();
```

---

## WebSocket & Streaming Issues

### WebSocket Connection Fails

**Cause**: Protocol upgrade not supported

**Ensure WebSocket middleware is configured**:
```rust
use axum::extract::ws::WebSocketUpgrade;

async fn ws_handler(
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        // Handle WebSocket messages
    })
}

let app = Router::new()
    .route("/ws", get(ws_handler));
```

**Common issue**: Missing `/ws` endpoint
```rust
// ❌ Only has /graphql, no WebSocket endpoint
let app = Router::new()
    .route("/graphql", post(graphql_handler));

// ✅ Add WebSocket support
let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .route("/graphql/subscriptions", get(ws_handler));
```

---

### WebSocket Message Loss

**Cause**: Buffer overflow or improper error handling

**Fix**:
```rust
use axum::extract::ws::{Message, WebSocket};

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Handle message
            }
            Ok(Message::Close(_)) => {
                // Gracefully close
                break;
            }
            Err(e) => {
                // Log error but don't panic
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}
```

---

## Security & Authentication Errors

### Error: "Invalid authorization header"

**Cause**: Token missing or malformed

**Diagnosis**:
```bash
# Test with valid token
curl -H "Authorization: Bearer <token>" \
     http://localhost:8000/graphql
```

**Common causes**:

1. **Missing token**
```bash
# ❌ Missing Bearer prefix
curl -H "Authorization: token123" ...

# ✅ Correct format
curl -H "Authorization: Bearer token123" ...
```

2. **Expired token**
```rust
// Check token expiration
let claims = decode::<Claims>(token, &key, &Validation::default())?;
if claims.claims.exp < SystemTime::now()
    .duration_since(UNIX_EPOCH)?
    .as_secs()
{
    return Err("Token expired");
}
```

3. **Invalid signature**
```rust
// Ensure same secret used for encoding and decoding
let secret = env::var("JWT_SECRET")?;
let key = DecodingKey::from_secret(secret.as_bytes());
```

---

### CORS Errors

**Symptom**: Browser shows "CORS error", request blocked

**Diagnosis**:
```bash
# Check CORS headers
curl -i -X OPTIONS http://localhost:8000/graphql \
  -H "Origin: http://localhost:3000" \
  -H "Access-Control-Request-Method: POST"
```

**Look for**:
```
Access-Control-Allow-Origin: http://localhost:3000
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Authorization
```

**If missing, CORS not configured properly**:
```rust
// ✅ Add CORS layer
let cors = CorsLayer::new()
    .allow_origin("http://localhost:3000".parse()?)
    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
    .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

let app = Router::new()
    .layer(cors);
```

---

## Logging & Debugging

### Enable Debug Logging

```bash
# Set log level
RUST_LOG=debug cargo run

# Log only fraiseql crate
RUST_LOG=fraiseql_rs=debug cargo run

# Log specific modules
RUST_LOG=fraiseql_rs::db=trace cargo run
```

### Initialize Logger

```rust
use tracing_subscriber;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Your server code...
}
```

### Request Logging Middleware

```rust
use axum::middleware;
use tower_http::trace::TraceLayer;

let app = Router::new()
    .layer(TraceLayer::new_for_http());
```

### Custom Debug Handler

```rust
async fn debug_info() -> impl IntoResponse {
    Json(json!({
        "uptime_seconds": uptime(),
        "requests_total": metrics.total_requests,
        "pool_size": pool.size(),
        "pool_idle": pool.num_idle(),
        "memory_mb": memory_usage_mb(),
    }))
}

let app = Router::new()
    .route("/debug", get(debug_info));
```

---

## Common Error Messages

### "error handling timeout"

**Cause**: Operation took longer than timeout

**Increase timeout**:
```rust
use std::time::Duration;
use tower::timeout::TimeoutLayer;

let timeout = TimeoutLayer::new(Duration::from_secs(60));

let app = Router::new()
    .layer(timeout);
```

---

### "error encoding response"

**Cause**: Response cannot be serialized to JSON

**Likely issue**: Type doesn't implement `Serialize`

**Fix**:
```rust
use serde::Serialize;

// ✅ Add Serialize derive
#[derive(Serialize)]
struct Response {
    id: i64,
    name: String,
}
```

---

### "error invalid request"

**Cause**: Request malformed

**Check**:
1. Valid JSON in body
2. Correct Content-Type header (`application/json`)
3. Valid GraphQL query

```bash
# Test with curl
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'
```

---

## Debugging Strategies

### Strategy 1: Minimal Reproduction

Create smallest possible example reproducing issue:
```rust
// Minimal example
async fn reproduce_issue() {
    let pool = create_pool().await.unwrap();
    let result = execute_simple_query(&pool).await;
    assert!(result.is_ok());
}
```

---

### Strategy 2: Binary Search

Narrow down where issue occurs:
```rust
// Add logging at key points
log::info!("Step 1: Connected to database");
log::info!("Step 2: Parsed GraphQL query");
log::info!("Step 3: Executed query");
log::info!("Step 4: Serialized response");

// See which step fails/slows down
```

---

### Strategy 3: Compare with Working Version

If worked before, see what changed:
```bash
git log --oneline -10
git diff <working-commit>
```

---

## Getting Help

### When Stuck

1. **Check the logs**: `RUST_LOG=debug cargo run`
2. **Read error message carefully**: Rust errors are descriptive
3. **Search for error**: Google "[your error message]"
4. **Check documentation**: [docs.rs/axum](https://docs.rs/axum)
5. **Ask community**: Rust forums, Discord channels

### Useful Resources

- **Axum Docs**: https://docs.rs/axum/latest/axum/
- **Tokio Docs**: https://tokio.rs/
- **Rust Book**: https://doc.rust-lang.org/book/
- **FraiseQL Issues**: GitHub issues page

### Creating a Minimal Example

When asking for help, provide:
```rust
// Simplified code showing the problem
#[tokio::main]
async fn main() {
    // Minimal reproduction
    // Should compile and fail with your issue
}
```

---

## Troubleshooting Checklist

When something breaks:

- [ ] Read error message carefully
- [ ] Enable debug logging (`RUST_LOG=debug`)
- [ ] Check if similar issue happened before (`git log`)
- [ ] Create minimal reproduction case
- [ ] Check recent changes
- [ ] Try reverting recent changes
- [ ] Check external services (database, cache)
- [ ] Ask for help with minimal example

---

## Next Steps

- **Need performance help?** → [Performance Tuning](./04-performance.md)
- **Back to Deployment?** → [Production Deployment](./03-deployment.md)
- **Back to Configuration?** → [Configuration](./02-configuration.md)
- **Back to Getting Started?** → [Getting Started](./01-getting-started.md)

---

**Remember**: Most issues have a simple fix. Read the error, enable logging, and search for the error message. The Rust community is helpful! 🚀
