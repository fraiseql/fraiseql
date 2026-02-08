# Phase 16: Native Rust HTTP Server with Axum

**Status**: Implementation Ready
**Target Version**: FraiseQL v2.0
**Total Effort**: 3-5 days (8 commits, ~800 lines of code)
**Framework**: Axum (Tokio's official web framework)

---

## 🎯 Executive Summary

Replace the Python HTTP layer (FastAPI/uvicorn) with a native Rust HTTP server built on **Axum**, maintaining 100% backward compatibility with the Python API. Axum is built on Tokio (our existing async runtime from Phase 15b) and provides type-safe routing, WebSocket support, and production-ready features.

### Performance Goals
- **Response Time**: <5ms for cached queries (vs 7-12ms with FastAPI)
- **Startup Time**: <100ms
- **Memory Usage**: <50MB idle
- **Concurrency**: 10,000+ concurrent connections
- **Overall Improvement**: 1.5-3x faster than Phase 15b

### Why Axum Over Custom HTTP
- ✅ Built on Tokio (no performance penalty)
- ✅ Proven pattern (Parviocula reference implementation)
- ✅ Type-safe routing at compile-time
- ✅ WebSocket support tested with Phase 15b subscriptions
- ✅ Middleware ecosystem (compression, CORS, rate limiting)
- ✅ 3-5 days instead of 2-3 weeks
- ✅ Lower risk (production-grade framework by Tokio team)

---

## 📊 Current Architecture

### Today (Phases 1-15)

```
Request from client
    ↓
[uvicorn - Python ASGI server]
    ↓
[FastAPI - Python HTTP router]
    ↓
[Python request parsing/validation]
    ↓
[Rust GraphQL Pipeline] ← Does 95% of the work
    ├── Query parsing
    ├── SQL generation
    ├── Cache lookup
    ├── Auth/RBAC/Security
    ├── Query execution
    └── Response building
    ↓
[Python JSON encoder]
    ↓
[uvicorn - Python ASGI response handler]
    ↓
Response to client
```

### After Phase 16 (with Axum)

```
Request from client
    ↓
[Axum HTTP Server] ← New: Replaces uvicorn + FastAPI
    ├── Accept connection
    ├── Type-safe routing
    └── Request extraction (JSON)
    ↓
[Axum Request Handler]
    ├── Extract JSON body
    ├── Validate request
    └── Build GraphQL request
    ↓
[Rust GraphQL Pipeline] ← Unchanged from Phases 1-15
    ├── Query parsing
    ├── SQL generation
    ├── Cache lookup
    ├── Auth/RBAC/Security
    ├── Query execution
    └── Response building (returns bytes)
    ↓
[Axum Response Handler]
    ├── Status code (200, 400, 500, etc.)
    ├── Headers (Content-Type, Cache-Control)
    └── JSON response (no Python encoding)
    ↓
Response to client
```

**Key difference**: No Python in the request path. Pure Rust all the way.

---

## 🏗️ Implementation Plan: 8 Commits

### Commit 1: Update Cargo.toml & Module Structure (1 hour)

**Files**:
- `fraiseql_rs/Cargo.toml` - Add Axum dependencies
- `fraiseql_rs/src/http/mod.rs` - Module structure

**Dependencies**:
```toml
axum = "0.7"                    # Web framework
tower = "0.4"                   # Middleware
tower-http = { version = "0.5", features = ["cors", "compression"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.35", features = ["full"] }
```

**What we keep from Commit 1 (custom HTTP)**:
- Delete: `http/server.rs` (replace with Axum)
- Keep: Core connection management concepts

---

### Commit 2: Basic Axum Server & GraphQL Handler (1-2 hours)

**Files**:
- `fraiseql_rs/src/http/axum_server.rs` - Axum HTTP server
- `fraiseql_rs/src/http/handlers.rs` - GraphQL request handler

**Key code**:
```rust
use axum::{
    routing::post,
    Json, Router, State,
};

pub struct HttpServerConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
}

pub async fn graphql_handler(
    State(pipeline): State<Arc<GraphQLPipeline>>,
    Json(request): Json<GraphQLRequest>,
) -> Json<GraphQLResponse> {
    // Execute GraphQL query
    pipeline.execute(request).await
}

pub async fn start_server(config: HttpServerConfig, pipeline: Arc<GraphQLPipeline>) {
    let app = Router::new()
        .route("/graphql", post(graphql_handler))
        .with_state(pipeline);

    let listener = tokio::net::TcpListener::bind(
        format!("{}:{}", config.host, config.port)
    ).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
```

**Tests**:
- Server starts on configured port
- GraphQL query returns valid response
- Connection tracking

---

### Commit 3: WebSocket & Subscriptions (1-2 hours)

**Files**:
- `fraiseql_rs/src/http/websocket.rs` - WebSocket handler

**Key code**:
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};

pub async fn subscriptions_handler(
    State(pipeline): State<Arc<GraphQLPipeline>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_subscription(pipeline, socket))
}

async fn handle_subscription(pipeline: Arc<GraphQLPipeline>, mut socket: WebSocket) {
    // Reuse Phase 15b subscription logic
    // Convert WebSocket frames to subscription protocol
    // Send updates back through socket
}
```

**Routing**:
```rust
app.route("/graphql/subscriptions", get(subscriptions_handler))
```

**Tests**:
- WebSocket upgrade works
- Subscription messages flow correctly
- Connection cleanup on disconnect

---

### Commit 4: Middleware & Error Handling (1-2 hours)

**Files**:
- `fraiseql_rs/src/http/middleware.rs` - Custom middleware
- `fraiseql_rs/src/http/errors.rs` - Error handling

**Middleware included**:
- Compression (gzip)
- CORS headers
- Request logging
- Error formatting

**Key code**:
```rust
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;

let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .route("/graphql/subscriptions", get(subscriptions_handler))
    .layer(CompressionLayer::new())
    .layer(CorsLayer::permissive())
    .with_state(pipeline);
```

**Error Handling**:
```rust
pub enum GraphQLError {
    ParseError(String),
    ExecutionError(String),
    ValidationError(String),
}

impl IntoResponse for GraphQLError {
    fn into_response(self) -> Response {
        let body = json!({
            "errors": [{
                "message": self.message(),
                "extensions": {
                    "code": self.error_code()
                }
            }]
        });
        (StatusCode::OK, Json(body)).into_response()
    }
}
```

**Tests**:
- Errors return correct status codes
- Error messages formatted correctly
- Middleware applied in right order

---

### Commit 5: Request Validation & Rate Limiting (1 hour)

**Files**:
- `fraiseql_rs/src/http/validation.rs` - Request validation
- `fraiseql_rs/src/http/rate_limit.rs` - Rate limiting

**Key features**:
- Validate GraphQL request structure
- Check query complexity
- Rate limiting per IP/user
- Query size limits

**Code**:
```rust
pub async fn graphql_handler(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<GraphQLRequest>,
) -> Result<Json<GraphQLResponse>, GraphQLError> {
    // Validate request
    request.validate()?;

    // Check rate limit
    state.rate_limiter.check_limit(addr.ip())?;

    // Execute
    Ok(Json(state.pipeline.execute(request).await))
}
```

**Tests**:
- Invalid queries rejected
- Rate limit enforced
- Query complexity limits work

---

### Commit 6: Connection Management & Monitoring (1-2 hours)

**Files**:
- `fraiseql_rs/src/http/connection.rs` - Connection tracking
- `fraiseql_rs/src/http/metrics.rs` - Metrics & monitoring

**Key metrics**:
- Active connections
- Requests per second
- Average latency
- Error rate
- Cache hit rate

**Code**:
```rust
pub struct ConnectionMetrics {
    active_connections: Arc<AtomicUsize>,
    requests_total: Arc<AtomicU64>,
    errors_total: Arc<AtomicU64>,
    latency_histogram: Arc<Histogram>,
}

pub async fn graphql_handler(
    State(state): State<AppState>,
    Json(request): Json<GraphQLRequest>,
) -> Result<Json<GraphQLResponse>, GraphQLError> {
    let start = Instant::now();
    state.metrics.active_connections.fetch_add(1, Ordering::Relaxed);

    let result = state.pipeline.execute(request).await;

    let elapsed = start.elapsed();
    state.metrics.latency_histogram.record(elapsed);
    state.metrics.active_connections.fetch_sub(1, Ordering::Relaxed);

    Ok(Json(result))
}
```

**Tests**:
- Metrics recorded correctly
- Connection count accurate
- Latency histogram works

---

### Commit 7: Python Bridge & PyO3 Bindings (2-3 hours)

**Files**:
- `src/fraiseql/http/` - Python module (new)
  - `__init__.py` - Module exports
  - `config.py` - Configuration
  - `server.py` - Server wrapper
- `fraiseql_rs/src/http/py_bindings.rs` - PyO3 bindings

**Python API** (unchanged from original plan):
```python
from fraiseql.http import create_rust_http_app, RustHttpConfig

config = RustHttpConfig(
    host="0.0.0.0",
    port=8000,
    max_connections=10000,
)

app = create_rust_http_app(schema=schema, config=config)
await app.start()
```

**PyO3 bindings**:
```rust
#[pyclass]
pub struct PyAxumServer {
    config: HttpServerConfig,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PyAxumServer {
    #[new]
    fn new(config: PyDict) -> PyResult<Self> {
        // Convert Python dict to Rust config
        Ok(Self {
            config: parse_config(config)?,
            runtime: Arc::new(Runtime::new()?),
        })
    }

    fn start(&mut self, py: Python) -> PyResult<&PyAny> {
        let runtime = Arc::clone(&self.runtime);
        let config = self.config.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            start_server(config).await;
            Ok(())
        })
    }

    fn shutdown(&mut self) {
        // Graceful shutdown
    }

    fn active_connections(&self) -> usize {
        // Return active connection count
    }
}
```

**Tests**:
- Server starts from Python
- Server shutdown works
- Configuration applied correctly

---

### Commit 8: Tests & Documentation (2-3 hours)

**Files**:
- `tests/unit/http/` - Rust unit tests
- `tests/integration/http/` - Python integration tests
- `docs/PHASE-16-AXUM.md` - Documentation

**Unit Tests** (Rust):
```rust
#[tokio::test]
async fn test_graphql_request() {
    let app = create_test_app().await;
    let client = TestClient::new(app);

    let response = client
        .post("/graphql")
        .json(&json!({
            "query": "{ user { id name } }"
        }))
        .send()
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.json().await["data"].is_object());
}

#[tokio::test]
async fn test_websocket_subscription() {
    let app = create_test_app().await;
    let client = TestClient::new(app);

    let ws = client.get("/graphql/subscriptions").upgrade().await.unwrap();
    // Send subscription, verify updates
}

#[tokio::test]
async fn test_rate_limiting() {
    // Verify rate limiting works
}

#[tokio::test]
async fn test_error_handling() {
    // Verify error responses formatted correctly
}
```

**Integration Tests** (Python):
```python
@pytest.mark.asyncio
async def test_server_starts():
    config = RustHttpConfig(port=9999)
    server = create_rust_http_app(schema=schema, config=config)
    await server.start()
    assert server.is_running

@pytest.mark.asyncio
async def test_graphql_query():
    async with httpx.AsyncClient() as client:
        response = await client.post(
            "http://localhost:8000/graphql",
            json={"query": "{ user { id } }"}
        )
        assert response.status_code == 200

@pytest.mark.asyncio
async def test_websocket_subscription():
    async with httpx.WebSocketClient("ws://localhost:8000/graphql/subscriptions") as ws:
        # Send subscription, verify updates
```

**Documentation**:
- Architecture overview
- Migration guide from FastAPI
- Performance comparison
- Configuration options
- Troubleshooting guide

---

## 📋 Key Differences from Custom HTTP Plan

| Aspect | Custom HTTP | Axum |
|--------|-------------|------|
| **Total commits** | 15 | 8 |
| **Lines of code** | ~3,000 | ~800 |
| **Manual parsing** | Yes (Commits 2) | No (built-in) |
| **Manual routing** | Yes (Commit 3) | No (type-safe) |
| **Error handling** | Custom (Commit 6) | Axum built-in |
| **Middleware** | None planned | CORS, compression, logging |
| **WebSocket** | Custom (Commits 7-9) | Axum built-in |
| **Timeline** | 2-3 weeks | 3-5 days |
| **Risk level** | Educational risk | Production-ready |

---

## 🧪 Testing Strategy

### Unit Tests (Rust)
- Server initialization
- Route handling
- WebSocket upgrade
- Error responses
- Middleware application
- Rate limiting
- Connection tracking

**Expected coverage**: >95% of HTTP module

### Integration Tests (Python)
- Server starts/stops cleanly
- GraphQL queries work
- WebSocket subscriptions work
- Error responses match format
- Performance benchmarks
- Concurrent requests
- Connection limits

**Expected coverage**: All user-facing features

### Performance Tests
- Response time <5ms for cached queries
- Server startup <100ms
- Memory usage <50MB idle
- 10,000+ concurrent connections
- No memory leaks

### Comparison Tests
- Response identical to FastAPI
- Headers identical to FastAPI
- Error format identical to FastAPI
- Performance >1.5x FastAPI

---

## 🎯 Success Criteria

### Functional
- ✅ Server starts/stops cleanly
- ✅ GraphQL requests work (identical responses to FastAPI)
- ✅ WebSocket subscriptions work
- ✅ Error handling matches FastAPI behavior
- ✅ All 5991+ existing tests pass

### Performance
- ✅ Response time: <5ms for cached queries
- ✅ Server startup: <100ms
- ✅ Memory usage: <50MB idle
- ✅ Concurrency: 10,000+ connections
- ✅ 1.5-3x faster than Phase 15b

### Compatibility
- ✅ 100% backward compatible Python API
- ✅ No user code changes required
- ✅ Can switch back to FastAPI without changes

### Quality
- ✅ Zero clippy warnings
- ✅ Full test coverage (>95%)
- ✅ Comprehensive documentation
- ✅ No regressions in existing tests

---

## 📚 References

### Axum Documentation
- [Axum GitHub](https://github.com/tokio-rs/axum)
- [Axum Docs](https://docs.rs/axum/latest/axum/)
- [Axum Examples](https://github.com/tokio-rs/axum/tree/main/examples)

### Parviocula (Reference Implementation)
- [Parviocula GitHub](https://github.com/tristan/parviocula)
- [Parviocula Docs](https://lib.rs/crates/parviocula)

### Related Phases
- Phase 15b: Tokio driver & subscriptions (prerequisite ✅)
- Phase 17: HTTP/2 & optimizations (next)
- Phase 18: Advanced load balancing (future)

---

## 🚀 Rollout Plan

### Week 1: Development (Days 1-3)
- Commits 1-4: Core HTTP server with handlers
- Commits 5-6: Validation, rate limiting, monitoring
- Local testing and iteration

### Week 1: Python Bridge & Testing (Days 4-5)
- Commit 7: PyO3 bindings and Python module
- Commit 8: Full test suite and documentation
- Integration testing

### Week 2: Performance & Deployment
- Performance benchmarking
- Load testing
- Staging deployment
- Production rollout

### Feature Flag (Optional)
```python
# In config
FRAISEQL_HTTP_SERVER = "axum"  # or "fastapi"

# In app factory
if os.getenv("FRAISEQL_HTTP_SERVER") == "axum":
    from fraiseql.http import create_rust_http_app
    app = create_rust_http_app(schema)
else:
    from fraiseql import create_fraiseql_app
    app = create_fraiseql_app(schema)
```

---

## 📊 Comparison: FastAPI vs Axum

| Feature | FastAPI | Axum | Winner |
|---------|---------|------|--------|
| **Speed** | 12-22ms | 7-12ms | Axum |
| **Setup** | Easy | Easy | Tie |
| **Python API** | Yes | Yes | Tie |
| **Memory** | 100-150MB | <50MB | Axum |
| **Connections** | 1,000/s | 5,000/s | Axum |
| **WebSocket** | Yes | Yes | Tie |
| **Middleware** | Starlette | Tower | Axum |
| **Type Safety** | Dynamic | Static | Axum |
| **Production Ready** | Yes | Yes | Tie |
| **Maintenance** | Starlete team | Tokio team | Axum |

---

## 🔄 Fallback Strategy

If Axum HTTP server has issues:

```python
# Option 1: Feature flag
FRAISEQL_HTTP_SERVER = "fastapi"  # Revert to FastAPI

# Option 2: Code change
# from fraiseql import create_fraiseql_app  # Revert to FastAPI

# No database migration, no schema changes
# Users don't notice the switch
```

---

## ✅ Pre-Implementation Checklist

- [ ] Read Axum documentation
- [ ] Review Parviocula reference implementation
- [ ] Understand Axum routing and handlers
- [ ] Review PyO3 async patterns
- [ ] Set up feature branch
- [ ] Plan test strategy
- [ ] Schedule code review

---

## 🎬 Getting Started

### 1. Create Feature Branch
```bash
git checkout -b feature/phase-16-axum-http-server
```

### 2. Update Cargo.toml
Add Axum and dependencies (Commit 1)

### 3. Start with Commit 1
- Add `axum`, `tower`, `tower-http` crates
- Create HTTP module structure
- Write basic tests

### 4. Iterate Through Commits
- Each commit is independent
- Test after each commit
- Document as you go

### 5. Performance Testing
- Benchmark against FastAPI
- Load testing
- Memory profiling

---

## 📞 Common Questions

**Q: Won't Axum add overhead?**
A: No. Axum is built on Tokio (our async runtime). Its overhead is <1ms per request.

**Q: How do we integrate with Python?**
A: PyO3 bindings (Commit 7). Parviocula shows the pattern.

**Q: Can we still use subscriptions?**
A: Yes. Axum's WebSocket support integrates with Phase 15b logic.

**Q: What if we want custom middleware?**
A: Axum uses Tower middleware. Easy to write custom middleware.

**Q: Performance compared to FastAPI?**
A: 1.5-3x faster. Rust + Tokio vs Python + uvicorn.

---

**Version**: 2.0
**Date**: January 3, 2026
**Status**: Ready for Implementation
**Effort**: 3-5 days
**Next Action**: Start Commit 1
