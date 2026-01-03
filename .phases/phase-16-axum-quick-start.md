# Phase 16: Axum Quick Start Guide

**Status**: Ready to implement
**Duration**: 3-5 days
**Commits**: 8
**Framework**: Axum 0.7

---

## Quick Command Reference

```bash
# Start implementation
git checkout -b feature/phase-16-axum-http-server

# Commit 1: Setup
# - Add Axum to Cargo.toml
# - Create http module
# - Add basic tests

# Commit 2: GraphQL Handler
# - Create Axum router
# - Implement /graphql POST handler
# - Extract JSON request
# - Execute GraphQL
# - Return JSON response

# Commit 3: WebSocket
# - Add /graphql/subscriptions GET handler
# - Integrate Phase 15b subscription logic
# - Handle WebSocket frames

# Commit 4: Middleware
# - Add CompressionLayer
# - Add CorsLayer
# - Custom error handler
# - Error formatting

# Commit 5: Validation
# - Request validation
# - Query complexity limits
# - Rate limiting

# Commit 6: Monitoring
# - Connection tracking
# - Metrics collection
# - Latency histogram

# Commit 7: PyO3 Bridge
# - Python module structure
# - PyO3 class bindings
# - Async wrapper

# Commit 8: Tests
# - Unit tests
# - Integration tests
# - Documentation
```

---

## Key Dependencies

Add to `fraiseql_rs/Cargo.toml`:

```toml
[dependencies]
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "compression", "trace"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.35", features = ["full"] }
hyper = "1.1"
futures = "0.3"
```

---

## Core Architecture

### Request Flow

```
HTTP Request
    ↓
Axum Router (type-safe)
    ↓
Handler Function
    ├─ Extract JSON (serde auto-deserialization)
    ├─ Validate request
    ├─ Check rate limit
    ↓
GraphQL Pipeline (Phase 1-15)
    ↓
Handler returns Response
    ├─ Status code
    ├─ Headers
    ├─ JSON body
    ↓
HTTP Response
```

### Basic Handler

```rust
use axum::{
    Router, Json, routing::post, State, extract::ConnectInfo,
};
use std::net::SocketAddr;
use std::sync::Arc;

pub async fn graphql_handler(
    State(pipeline): State<Arc<GraphQLPipeline>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<GraphQLRequest>,
) -> Json<GraphQLResponse> {
    pipeline.execute(request, addr.ip()).await
}

pub fn create_router(pipeline: Arc<GraphQLPipeline>) -> Router {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .with_state(pipeline)
}
```

---

## Axum Concepts Quick Reference

### Router & Routes
```rust
// Create router with POST handler
let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .route("/graphql/subscriptions", get(ws_handler));

// Add middleware
let app = app
    .layer(CompressionLayer::new())
    .layer(CorsLayer::permissive());
```

### Extractors (Auto JSON parsing)
```rust
// Axum automatically deserializes JSON
async fn handler(
    Json(request): Json<GraphQLRequest>,  // ← Auto JSON parse
) -> Json<GraphQLResponse> {
    // request is already parsed!
}
```

### State Management
```rust
// Pass data to handlers via State
.route("/graphql", post(graphql_handler))
.with_state(Arc::new(pipeline))

// Access in handler
async fn handler(
    State(pipeline): State<Arc<GraphQLPipeline>>,
) { ... }
```

### WebSocket
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket))
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        // Handle subscription messages
    }
}
```

### Error Handling
```rust
use axum::response::IntoResponse;

pub enum GraphQLError {
    Parse(String),
    Execution(String),
}

impl IntoResponse for GraphQLError {
    fn into_response(self) -> Response {
        let body = json!({ "errors": [{ "message": self.message() }] });
        (StatusCode::OK, Json(body)).into_response()
    }
}
```

### Middleware
```rust
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;

let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .layer(CompressionLayer::new())           // Auto gzip
    .layer(CorsLayer::permissive())           // CORS headers
    .with_state(pipeline);
```

---

## Testing Patterns

### Unit Test
```rust
#[tokio::test]
async fn test_graphql_handler() {
    let app = create_router(Arc::new(test_pipeline()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/graphql")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"{ test }"}"#))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

### Integration Test (Python)
```python
import pytest
import httpx

@pytest.mark.asyncio
async def test_graphql_query():
    async with httpx.AsyncClient() as client:
        response = await client.post(
            "http://localhost:8000/graphql",
            json={"query": "{ user { id } }"}
        )
        assert response.status_code == 200
```

---

## Performance Tips

1. **Use State<Arc<T>>** for zero-copy data sharing
2. **Compress responses** with CompressionLayer
3. **Cache GraphQL schema** in State
4. **Use connection pooling** for database
5. **Monitor with metrics** (Prometheus-compatible)

---

## Common Patterns

### Extract IP Address
```rust
async fn handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) {
    let ip = addr.ip();
}
```

### Check Request Headers
```rust
async fn handler(
    headers: HeaderMap,
) {
    let auth = headers.get("authorization");
}
```

### Nested Routes
```rust
let graphql_routes = Router::new()
    .route("/", post(graphql_handler))
    .route("/subscriptions", get(ws_handler));

let app = Router::new()
    .nest("/graphql", graphql_routes);
```

---

## Debugging

### Enable logging
```rust
use tracing::info;

info!("GraphQL query received: {:?}", request);
```

### Add tower trace middleware
```rust
use tower_http::trace::TraceLayer;

let app = app.layer(TraceLayer::new_for_http());
```

### Print request/response
```rust
async fn handler(Json(req): Json<GraphQLRequest>) -> Json<GraphQLResponse> {
    eprintln!("Request: {:?}", req);
    let resp = execute(req).await;
    eprintln!("Response: {:?}", resp);
    Json(resp)
}
```

---

## References

- **Axum Docs**: https://docs.rs/axum/latest/axum/
- **Axum Book**: https://github.com/tokio-rs/axum/tree/main/examples
- **Tower Middleware**: https://docs.rs/tower/latest/tower/
- **Tokio Tutorial**: https://tokio.rs/

---

## Commit Checklist

### Commit 1
- [ ] Add Axum dependencies to Cargo.toml
- [ ] Create `fraiseql_rs/src/http/mod.rs`
- [ ] Cargo check passes
- [ ] Write module-level tests

### Commit 2
- [ ] Create `fraiseql_rs/src/http/axum_server.rs`
- [ ] Implement basic handler
- [ ] Test request parsing
- [ ] Test response formatting

### Commit 3
- [ ] Add WebSocket handler
- [ ] Integrate Phase 15b subscription logic
- [ ] Test WebSocket messages

### Commit 4
- [ ] Add compression middleware
- [ ] Add CORS middleware
- [ ] Implement error handler
- [ ] Test error formatting

### Commit 5
- [ ] Add request validation
- [ ] Add rate limiter
- [ ] Test validation errors
- [ ] Test rate limit rejection

### Commit 6
- [ ] Add connection tracking
- [ ] Implement metrics
- [ ] Test metrics collection

### Commit 7
- [ ] Create `src/fraiseql/http/` module
- [ ] Write PyO3 bindings
- [ ] Test Python API

### Commit 8
- [ ] Write comprehensive tests
- [ ] Benchmark performance
- [ ] Write documentation

---

**Ready to start?** Begin with Commit 1!
