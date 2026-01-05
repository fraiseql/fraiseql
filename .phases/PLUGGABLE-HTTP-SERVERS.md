# Pluggable HTTP Servers Architecture Plan

**Phase**: Feature/Architecture Design
**Target Version**: v2.0.0
**Status**: Planning
**Last Updated**: January 5, 2026

---

## 🎯 Objective

Design and implement a **pluggable HTTP server architecture** that allows FraiseQL to support multiple HTTP frameworks as interchangeable implementations while maintaining:
- **Axum** as the primary, performance-optimized implementation (Rust)
- **Starlette** as the Python-native alternative for Python-first deployments
- **FastAPI** as a convenience wrapper (compatibility layer)
- **Single business logic**: Core framework features are framework-agnostic
- **Consistent behavior**: All implementations produce identical results

---

## 📋 Context

### Current State
- FastAPI: 64KB, deeply integrated, becoming maintenance burden
- Axum: In `Cargo.toml`, but not yet fully implemented as HTTP server
- Rust pipeline: Mature, optimized, 7-10x faster than Python JSON handling
- Test suite: 5991+ tests, mostly framework-agnostic

### Problem
- Two HTTP implementations (FastAPI + Axum) causing maintenance burden
- No clear hierarchy—features need to be implemented twice
- Drift risk: APQ caching, auth, middleware can diverge
- Users confused about which to use

### Opportunity
- Rust ecosystem (Axum) is 2025-forward
- Python ecosystem (Starlette) is stable, proven
- Can use same core framework code for both
- Abstract HTTP layer enables future frameworks (Quart, FastAPI 1.0, etc.)

---

## 🏗️ Architecture Design

### Core Principle: Pluggable HTTP Servers

```
┌─────────────────────────────────────────────────────┐
│ HTTP Server Layer (Pluggable)                       │
│                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │   Axum       │  │  Starlette   │  │ FastAPI  │  │
│  │   (Primary)  │  │  (Python)    │  │ (Compat) │  │
│  └──────────────┘  └──────────────┘  └──────────┘  │
└─────────────────────────────────────────────────────┘
           ↓              ↓                ↓
┌─────────────────────────────────────────────────────┐
│ HTTP Server Abstraction Layer                       │
│                                                     │
│  interface HttpServer {                            │
│    - route()                                        │
│    - middleware()                                   │
│    - context()                                      │
│    - response_builder()                             │
│  }                                                   │
└─────────────────────────────────────────────────────┘
           ↓              ↓                ↓
┌─────────────────────────────────────────────────────┐
│ Core Framework Layer (Framework-Agnostic)          │
│                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │  Middleware  │  │ Executors    │  │ APQ,     │  │
│  │  (APQ, Auth) │  │ (Rust+Python)│  │ IDPolicy │  │
│  └──────────────┘  └──────────────┘  └──────────┘  │
└─────────────────────────────────────────────────────┘
           ↓              ↓                ↓
┌─────────────────────────────────────────────────────┐
│ Rust Pipeline Layer (fraiseql_rs)                   │
│                                                     │
│  - JSON transformation (7-10x faster)              │
│  - Query execution                                  │
│  - Caching                                          │
└─────────────────────────────────────────────────────┘
           ↓
┌─────────────────────────────────────────────────────┐
│ PostgreSQL (Database Layer)                         │
└─────────────────────────────────────────────────────┘
```

### Implementation Layers

#### 1. **HTTP Server Abstraction** (New)
Framework-agnostic interface that all HTTP servers implement:

```python
# src/fraiseql/http/interface.py

from typing import Protocol, Any, Callable, Awaitable
from dataclasses import dataclass

@dataclass
class HttpContext:
    """Framework-agnostic HTTP context"""
    request_body: dict[str, Any]
    headers: dict[str, str]
    user: Any | None
    variables: dict[str, Any] | None
    operation_name: str | None

@dataclass
class HttpResponse:
    """Framework-agnostic HTTP response"""
    status_code: int
    body: dict[str, Any] | str
    headers: dict[str, str] | None = None

class HttpServer(Protocol):
    """Interface for pluggable HTTP servers"""

    async def handle_graphql(self, context: HttpContext) -> HttpResponse:
        """Execute GraphQL request and return response"""
        ...

    async def handle_health(self) -> HttpResponse:
        """Health check endpoint"""
        ...

    async def handle_introspection(self, context: HttpContext) -> HttpResponse:
        """GraphQL introspection"""
        ...

    async def handle_subscriptions(self, context: HttpContext) -> AsyncIterator[HttpResponse]:
        """WebSocket subscriptions"""
        ...

    def add_middleware(self, middleware: Callable) -> None:
        """Add framework-specific middleware"""
        ...

    def add_route(self, path: str, handler: Callable) -> None:
        """Add custom route"""
        ...
```

#### 2. **Framework-Agnostic Core** (Existing + Refactor)
Move business logic out of FastAPI:

```
src/fraiseql/
├── http/                           # NEW
│   ├── __init__.py
│   ├── interface.py                # HttpServer protocol
│   ├── context_builder.py          # Extract request → context
│   ├── response_builder.py         # Response object → framework response
│   └── handlers/
│       ├── __init__.py
│       ├── graphql.py              # GraphQL execution (framework-agnostic)
│       ├── health.py               # Health check
│       ├── introspection.py        # Introspection
│       └── subscriptions.py        # WebSocket handling
├── fastapi/                        # REFACTORED (compatibility only)
│   ├── routers.py                  # FastAPI wrappers
│   └── app.py                      # FastAPI app setup
├── starlette/                      # NEW
│   ├── __init__.py
│   ├── app.py                      # Starlette app setup
│   └── middleware.py               # Starlette-specific middleware
└── axum/                           # NEW (Python stubs for Rust)
    ├── __init__.py
    └── py_bindings.pyi             # Type stubs for Rust implementation
```

#### 3. **Axum Implementation** (Rust)
Rust layer implements full HTTP server:

```
fraiseql_rs/
├── src/
│   ├── http/                       # NEW
│   │   ├── mod.rs
│   │   ├── server.rs               # Axum app setup
│   │   ├── handlers.rs             # Route handlers
│   │   ├── middleware.rs           # Axum middleware
│   │   └── context.rs              # Request context building
│   └── py_bindings.rs              # PyO3 bindings for HTTP server
└── Cargo.toml                       # Already has axum = "0.7"
```

---

## 📊 Phase Breakdown

### Phase 0: Analysis & Design (Week 1)
**Objective**: Finalize architecture, document decisions

**Deliverables**:
- [ ] Detailed HTTP server interface spec
- [ ] Middleware abstraction design
- [ ] Response builder protocol
- [ ] WebSocket/subscriptions strategy
- [ ] Migration path documentation

**Key Decisions to Make**:
1. **Framework Detection**: Should users specify server at startup, or auto-detect?
2. **Middleware Order**: How are middleware stacked across frameworks?
3. **WebSocket**: Which servers support it? (Axum yes, Starlette yes, FastAPI yes)
4. **Error Handling**: Unified error response format?

**Files to Create**:
- `.phases/PLUGGABLE-HTTP-SERVERS.md` (this file)
- `docs/architecture/http-servers.md` - Architecture documentation
- `docs/guides/choosing-http-server.md` - Selection guide for users

---

### Phase 1: HTTP Server Abstraction Layer (Week 2-3)
**Objective**: Create framework-agnostic interfaces and extract business logic

**TDD Cycle**: RED → GREEN → REFACTOR → QA

#### 1.1: RED - Write Tests for Abstraction
```python
# tests/unit/http/test_http_interface.py

class TestHttpServerInterface:
    """Test that all HTTP servers implement identical interface"""

    async def test_graphql_request_handling(self):
        """All servers handle GraphQL requests identically"""
        context = HttpContext(
            request_body={"query": "{ __typename }"},
            headers={"authorization": "Bearer token"},
            user=None,
            variables=None,
            operation_name=None,
        )

        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            response = await server.handle_graphql(context)
            assert response.status_code == 200
            assert "data" in response.body

    async def test_error_response_format(self):
        """All servers return errors in same format"""
        context = HttpContext(
            request_body={"query": "{ invalid }"},
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            response = await server.handle_graphql(context)
            assert response.status_code == 400
            assert "errors" in response.body

    async def test_health_check(self):
        """All servers have identical health endpoint"""
        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            response = await server.handle_health()
            assert response.status_code == 200
            assert "status" in response.body

    async def test_introspection(self):
        """All servers support introspection"""
        context = HttpContext(
            request_body={"query": INTROSPECTION_QUERY},
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            response = await server.handle_graphql(context)
            assert response.status_code == 200
            assert "__schema" in response.body["data"]

    async def test_apq_caching(self):
        """APQ caching works identically across all servers"""
        # First request: full query
        context1 = HttpContext(
            request_body={
                "query": "{ user { id name } }",
                "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
            },
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        # Second request: hash only
        context2 = HttpContext(
            request_body={
                "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
            },
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            resp1 = await server.handle_graphql(context1)
            resp2 = await server.handle_graphql(context2)

            # Both should succeed
            assert resp1.status_code == 200
            assert resp2.status_code == 200

            # Results should be identical
            assert resp1.body["data"] == resp2.body["data"]

    async def test_middleware_execution(self):
        """Middleware executes in same order for all servers"""
        execution_order = []

        def middleware1(next_handler):
            async def handler(context):
                execution_order.append("middleware1_before")
                response = await next_handler(context)
                execution_order.append("middleware1_after")
                return response
            return handler

        def middleware2(next_handler):
            async def handler(context):
                execution_order.append("middleware2_before")
                response = await next_handler(context)
                execution_order.append("middleware2_after")
                return response
            return handler

        context = HttpContext(
            request_body={"query": "{ __typename }"},
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            execution_order.clear()
            server.add_middleware(middleware1)
            server.add_middleware(middleware2)

            await server.handle_graphql(context)

            # Middleware should execute in FIFO order
            expected = ["middleware1_before", "middleware2_before", "middleware2_after", "middleware1_after"]
            assert execution_order == expected

    async def test_context_building(self):
        """Context is built consistently from framework requests"""
        # Test with authorization header
        context = HttpContext(
            request_body={"query": "{ viewer { id } }"},
            headers={"authorization": "Bearer user-token-123"},
            user=None,
            variables=None,
            operation_name=None,
        )

        for server in [AxumServer(), StarletteServer(), FastAPIServer()]:
            response = await server.handle_graphql(context)
            # Auth should be applied consistently
            assert response.status_code in [200, 401]
```

#### 1.2: GREEN - Implement HTTP Interface
```python
# src/fraiseql/http/interface.py

from typing import Protocol, Any, AsyncIterator, Callable, Awaitable
from dataclasses import dataclass
from enum import Enum

class HttpMethod(Enum):
    GET = "GET"
    POST = "POST"
    PUT = "PUT"
    DELETE = "DELETE"
    PATCH = "PATCH"

@dataclass
class HttpContext:
    """Framework-agnostic HTTP context"""
    request_body: dict[str, Any] | bytes
    headers: dict[str, str]
    user: Any | None = None
    variables: dict[str, Any] | None = None
    operation_name: str | None = None
    method: HttpMethod = HttpMethod.POST
    query_params: dict[str, str] | None = None

@dataclass
class HttpResponse:
    """Framework-agnostic HTTP response"""
    status_code: int
    body: dict[str, Any] | str | bytes
    headers: dict[str, str] | None = None
    content_type: str = "application/json"

class HttpServer(Protocol):
    """Interface for pluggable HTTP servers

    All implementations must provide identical behavior:
    - Same GraphQL results
    - Same error formats
    - Same middleware execution order
    - Same APQ caching behavior
    - Same authentication/authorization
    """

    async def handle_graphql(self, context: HttpContext) -> HttpResponse:
        """Execute GraphQL request

        Args:
            context: Framework-agnostic HTTP context

        Returns:
            HttpResponse with status, body, headers

        Raises:
            GraphQLError: If query is invalid
            AuthenticationError: If authentication fails
            PermissionError: If user lacks permission
        """
        ...

    async def handle_health(self) -> HttpResponse:
        """Health check endpoint

        Returns:
            {"status": "healthy", "version": "1.2.3"}
        """
        ...

    async def handle_introspection(self, context: HttpContext) -> HttpResponse:
        """GraphQL introspection

        Standard __schema query support
        """
        ...

    async def handle_subscriptions(
        self, context: HttpContext
    ) -> AsyncIterator[HttpResponse]:
        """WebSocket subscriptions

        Yields:
            HttpResponse objects as events arrive
        """
        ...

    def add_middleware(self, middleware: Callable[[Callable], Callable]) -> None:
        """Add middleware in FIFO order

        Middleware signature:
            async def middleware(context: HttpContext) -> HttpResponse
        """
        ...

    def add_route(
        self,
        path: str,
        handler: Callable[[HttpContext], Awaitable[HttpResponse]],
        methods: list[HttpMethod] | None = None,
    ) -> None:
        """Add custom route"""
        ...

    async def startup(self) -> None:
        """Server startup hook"""
        ...

    async def shutdown(self) -> None:
        """Server shutdown hook"""
        ...
```

#### 1.3: Extract Business Logic
```python
# src/fraiseql/http/handlers/graphql.py

async def execute_graphql_request(
    context: HttpContext,
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
    middleware_stack: list[Middleware] | None = None,
) -> HttpResponse:
    """Framework-agnostic GraphQL execution

    This is the single source of truth for GraphQL handling.
    All HTTP servers (Axum, Starlette, FastAPI) call this function.

    Handles:
    - Query parsing
    - APQ caching
    - Field selection filtering
    - Authentication/authorization
    - Middleware execution
    - Error formatting
    - Response caching

    Returns:
        HttpResponse with identical format regardless of HTTP server
    """
    try:
        # 1. Parse request
        query = context.request_body.get("query")
        variables = context.request_body.get("variables", {})
        operation_name = context.request_body.get("operationName")

        # 2. Check APQ cache
        apq_hash = context.request_body.get("extensions", {}).get("persistedQuery", {}).get("sha256Hash")
        if apq_hash and not query:
            query = load_from_apq_cache(apq_hash)
            if not query:
                return HttpResponse(
                    status_code=400,
                    body={"errors": [{"message": "Unknown operation hash"}]},
                )

        # 3. Execute middleware
        for middleware in middleware_stack or []:
            await middleware.before_execution(context)

        # 4. Execute GraphQL
        result = await execute_graphql(
            schema=schema,
            query=query,
            variables=variables,
            operation_name=operation_name,
            context_value=context,
        )

        # 5. Execute middleware after-hooks
        for middleware in reversed(middleware_stack or []):
            await middleware.after_execution(result)

        # 6. Format response
        response_body = {
            "data": result.data,
            "errors": [format_error(e) for e in result.errors] if result.errors else None,
        }

        # Remove None errors key
        if response_body["errors"] is None:
            del response_body["errors"]

        # 7. Cache response if APQ
        if apq_hash:
            cache_response(apq_hash, response_body)

        return HttpResponse(
            status_code=200 if not result.errors else 400,
            body=response_body,
            headers={"X-GraphQL-Cache": "HIT" if apq_hash else "MISS"},
        )

    except Exception as e:
        logger.error(f"GraphQL execution error: {e}")
        return HttpResponse(
            status_code=500,
            body={"errors": [{"message": str(e)}]},
        )
```

**Deliverables**:
- [ ] `src/fraiseql/http/interface.py` - HTTP server protocol
- [ ] `src/fraiseql/http/context_builder.py` - Request parsing
- [ ] `src/fraiseql/http/response_builder.py` - Response formatting
- [ ] `src/fraiseql/http/handlers/` - Business logic extraction
- [ ] `tests/unit/http/` - Complete test coverage
- [ ] All existing tests still pass

---

### Phase 2: Axum HTTP Server Implementation (Week 4-5)
**Objective**: Build complete Axum server as primary implementation

**TDD Cycle**: RED → GREEN → REFACTOR → QA

#### 2.1: RED - Write Axum Integration Tests
```python
# tests/integration/axum/test_axum_server.py

class TestAxumServer:
    """Test Axum HTTP server implementation"""

    @pytest.fixture
    async def axum_server(self):
        """Start Axum server in test mode"""
        server = AxumServer(config=FraiseQLConfig(...))
        await server.startup()
        yield server
        await server.shutdown()

    async def test_graphql_query(self, axum_server, client):
        """Axum handles GraphQL queries"""
        response = await client.post(
            "/graphql",
            json={"query": "{ __typename }"},
        )
        assert response.status_code == 200
        assert response.json()["data"]["__typename"] == "Query"

    async def test_apq_query(self, axum_server, client):
        """Axum handles APQ queries"""
        # First: Full query
        response = await client.post(
            "/graphql",
            json={
                "query": "{ user { id } }",
                "extensions": {
                    "persistedQuery": {
                        "version": 1,
                        "sha256Hash": "abc123",
                    }
                },
            },
        )
        assert response.status_code == 200

        # Second: Hash only
        response = await client.post(
            "/graphql",
            json={
                "extensions": {
                    "persistedQuery": {
                        "version": 1,
                        "sha256Hash": "abc123",
                    }
                },
            },
        )
        assert response.status_code == 200

    async def test_websocket_subscription(self, axum_server, client):
        """Axum handles WebSocket subscriptions"""
        with client.websocket_connect("/graphql") as websocket:
            # Subscribe
            await websocket.send_json({
                "id": "1",
                "type": "start",
                "payload": {"query": "subscription { userCreated { id } }"},
            })

            # Receive first message
            data = await websocket.receive_json()
            assert data["type"] == "data"
            assert "payload" in data

    async def test_health_endpoint(self, axum_server, client):
        """Axum health endpoint"""
        response = await client.get("/health")
        assert response.status_code == 200
        assert response.json()["status"] == "healthy"

    async def test_middleware_execution(self, axum_server, client):
        """Axum middleware executes correctly"""
        response = await client.post(
            "/graphql",
            json={"query": "{ __typename }"},
            headers={"X-Custom-Header": "test"},
        )
        assert response.status_code == 200
        assert "X-Custom-Middleware" in response.headers
```

#### 2.2: GREEN - Implement Axum Server
```rust
// fraiseql_rs/src/http/server.rs

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub schema: Arc<GraphQLSchema>,
    pub config: Arc<FraiseQLConfig>,
}

pub async fn build_axum_server(config: FraiseQLConfig) -> Router {
    let state = AppState {
        schema: Arc::new(build_schema(&config)),
        config: Arc::new(config),
    };

    Router::new()
        .route("/graphql", post(handle_graphql))
        .route("/graphql", get(handle_graphql_get))
        .route("/health", get(health_check))
        .route("/.well-known/apollo/server-health", get(health_check))
        .route("/introspect", get(introspection))
        .with_state(state)
        .layer(middleware::middleware_stack())
}

async fn handle_graphql(
    State(state): State<AppState>,
    Json(request): Json<GraphQLRequest>,
) -> Response {
    // Build framework-agnostic context
    let context = HttpContext {
        request_body: request.into(),
        headers: Default::default(),
        user: None,
        variables: None,
        operation_name: None,
    };

    // Call unified handler
    match execute_graphql_request(&context, &state.schema, &state.config).await {
        Ok(response) => response.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"errors": [{"message": e.to_string()}]})),
        )
            .into_response(),
    }
}

async fn health_check() -> Response {
    Json(json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
    }))
    .into_response()
}
```

**Deliverables**:
- [ ] `fraiseql_rs/src/http/` - Complete Axum implementation
- [ ] `fraiseql_rs/src/http/server.rs` - Main server setup
- [ ] `fraiseql_rs/src/http/handlers.rs` - Route handlers
- [ ] `fraiseql_rs/src/http/middleware.rs` - Middleware stack
- [ ] PyO3 bindings for Python integration
- [ ] `tests/integration/axum/` - All integration tests pass
- [ ] Performance benchmarks show 7-10x improvement over FastAPI

---

### Phase 3: Starlette HTTP Server Implementation (Week 6)
**Objective**: Implement Starlette-based server for Python-first deployments

**TDD Cycle**: RED → GREEN → REFACTOR → QA

#### 3.1: RED - Write Starlette Integration Tests
```python
# tests/integration/starlette/test_starlette_server.py

class TestStarletteServer:
    """Test Starlette HTTP server implementation"""

    @pytest.fixture
    async def starlette_server(self):
        """Start Starlette server in test mode"""
        app = create_starlette_app(FraiseQLConfig(...))
        async with TestClient(app) as client:
            yield client

    async def test_graphql_query(self, starlette_server):
        """Starlette handles GraphQL queries"""
        response = starlette_server.post(
            "/graphql",
            json={"query": "{ __typename }"},
        )
        assert response.status_code == 200
        assert response.json()["data"]["__typename"] == "Query"

    async def test_apq_query(self, starlette_server):
        """Starlette handles APQ queries"""
        # First: Full query
        response = starlette_server.post(
            "/graphql",
            json={
                "query": "{ user { id } }",
                "extensions": {
                    "persistedQuery": {
                        "version": 1,
                        "sha256Hash": "abc123",
                    }
                },
            },
        )
        assert response.status_code == 200

        # Second: Hash only
        response = starlette_server.post(
            "/graphql",
            json={
                "extensions": {
                    "persistedQuery": {
                        "version": 1,
                        "sha256Hash": "abc123",
                    }
                },
            },
        )
        assert response.status_code == 200

    async def test_websocket_subscription(self, starlette_server):
        """Starlette handles WebSocket subscriptions"""
        with starlette_server.websocket_connect("/graphql") as websocket:
            websocket.send_json({
                "id": "1",
                "type": "start",
                "payload": {"query": "subscription { userCreated { id } }"},
            })
            data = websocket.receive_json()
            assert data["type"] == "data"
```

#### 3.2: GREEN - Implement Starlette Server
```python
# src/fraiseql/starlette/app.py

from starlette.applications import Starlette
from starlette.responses import JSONResponse
from starlette.routing import Route, WebSocketRoute
from starlette.middleware import Middleware
from starlette.middleware.base import BaseHTTPMiddleware

async def graphql_endpoint(request):
    """GraphQL POST endpoint"""
    body = await request.json()

    # Convert Starlette request to framework-agnostic context
    context = HttpContext(
        request_body=body,
        headers=dict(request.headers),
        user=getattr(request, "user", None),
        variables=body.get("variables"),
        operation_name=body.get("operationName"),
    )

    # Execute using unified handler
    response = await execute_graphql_request(context, schema, config)

    return JSONResponse(response.body, status_code=response.status_code)

async def graphql_subscription(websocket):
    """GraphQL WebSocket subscription"""
    await websocket.accept()

    # ... subscription handling ...

def create_starlette_app(config: FraiseQLConfig) -> Starlette:
    """Create Starlette application

    Provides identical functionality to Axum server
    """
    return Starlette(
        routes=[
            Route("/graphql", graphql_endpoint, methods=["POST"]),
            WebSocketRoute("/graphql", graphql_subscription),
            Route("/health", health_check),
        ],
        middleware=[
            Middleware(GraphQLMiddleware),
            Middleware(AuthMiddleware),
            Middleware(APQMiddleware),
        ],
    )
```

**Deliverables**:
- [ ] `src/fraiseql/starlette/` - Complete Starlette implementation
- [ ] `src/fraiseql/starlette/app.py` - App setup
- [ ] `src/fraiseql/starlette/middleware.py` - Starlette middleware
- [ ] `tests/integration/starlette/` - All tests pass
- [ ] Performance benchmarks (baseline for Python-native)

---

### Phase 4: FastAPI Compatibility Layer (Week 7)
**Objective**: Convert FastAPI to thin wrapper around Starlette/Axum

**Strategy**: FastAPI becomes optional compatibility shim, not primary implementation

#### 4.1: Refactor FastAPI to Use Abstraction
```python
# src/fraiseql/fastapi/app.py - REFACTORED

from fraiseql.http.interface import HttpContext, HttpResponse
from fraiseql.http.handlers.graphql import execute_graphql_request
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

def create_fastapi_app(config: FraiseQLConfig) -> FastAPI:
    """Create FastAPI application

    DEPRECATED: Use Axum (recommended) or Starlette (Python-native)

    This is a compatibility layer that delegates to the unified HTTP handler.
    All business logic is framework-agnostic.
    """
    app = FastAPI(title="FraiseQL")

    @app.post("/graphql")
    async def graphql_endpoint(request: Request):
        body = await request.json()

        # Convert FastAPI request to framework-agnostic context
        context = HttpContext(
            request_body=body,
            headers=dict(request.headers),
            user=getattr(request.state, "user", None),
            variables=body.get("variables"),
            operation_name=body.get("operationName"),
        )

        # Delegate to unified handler
        response = await execute_graphql_request(context, schema, config)

        return JSONResponse(response.body, status_code=response.status_code)

    @app.get("/health")
    async def health_check():
        return {"status": "healthy", "version": __version__}

    return app
```

#### 4.2: Mark as Deprecated
- Add deprecation warnings to FastAPI module
- Update README and docs
- Add migration guide to Axum/Starlette
- Set deprecation timeline (v3.0: removal)

**Deliverables**:
- [ ] `src/fraiseql/fastapi/` - Refactored as thin wrapper
- [ ] Deprecation notices in code
- [ ] `docs/migration/fastapi-to-axum.md` - Migration guide
- [ ] Existing FastAPI tests still pass
- [ ] Clear path forward for FastAPI users

---

### Phase 5: Unified Testing & Documentation (Week 8)
**Objective**: Ensure all servers behave identically, document usage

#### 5.1: Unified Test Suite
```python
# tests/integration/test_all_http_servers.py

class TestHttpServerParity:
    """Test that ALL HTTP servers produce identical behavior"""

    @pytest.fixture(params=["axum", "starlette", "fastapi"])
    async def http_server(self, request):
        """Parametrized fixture testing all servers"""
        if request.param == "axum":
            server = AxumServer(config)
        elif request.param == "starlette":
            server = StarletteServer(config)
        else:  # fastapi
            server = FastAPIServer(config)

        await server.startup()
        yield server
        await server.shutdown()

    async def test_identical_graphql_results(self, http_server):
        """All servers produce identical GraphQL results"""
        queries = [
            "{ __typename }",
            "{ user { id name } }",
            "query GetUser($id: ID!) { user(id: $id) { id name } }",
        ]

        baseline_results = None
        for query in queries:
            context = HttpContext(request_body={"query": query}, ...)
            response = await http_server.handle_graphql(context)

            if baseline_results is None:
                baseline_results = response.body
            else:
                assert response.body == baseline_results

    async def test_identical_error_messages(self, http_server):
        """All servers format errors identically"""
        context = HttpContext(
            request_body={"query": "{ invalid }"},
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        response = await http_server.handle_graphql(context)
        assert response.status_code == 400
        assert "errors" in response.body

    async def test_identical_apq_behavior(self, http_server):
        """APQ caching works identically across servers"""
        # Test sequence: full query → hash-only → full query

        full_query_context = HttpContext(
            request_body={
                "query": "{ user { id } }",
                "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "abc"}},
            },
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        hash_only_context = HttpContext(
            request_body={
                "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "abc"}},
            },
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        # All servers should behave the same
        resp1 = await http_server.handle_graphql(full_query_context)
        resp2 = await http_server.handle_graphql(hash_only_context)
        resp3 = await http_server.handle_graphql(full_query_context)

        assert resp1.body == resp2.body == resp3.body
```

#### 5.2: Performance Benchmarks
```python
# tests/benchmarks/http_servers.py

class HttpServerBenchmarks:
    """Compare performance across HTTP servers"""

    @pytest.mark.benchmark
    def test_simple_query_performance(self, benchmark):
        """Measure query execution time for each server"""

        servers = {
            "axum": AxumServer(config),
            "starlette": StarletteServer(config),
            "fastapi": FastAPIServer(config),
        }

        context = HttpContext(
            request_body={"query": "{ __typename }"},
            headers={},
            user=None,
            variables=None,
            operation_name=None,
        )

        results = {}
        for name, server in servers.items():
            def execute():
                return asyncio.run(server.handle_graphql(context))

            result = benchmark(execute)
            results[name] = result

        # Axum should be 5-10x faster than Starlette
        axum_time = results["axum"].stats.mean
        starlette_time = results["starlette"].stats.mean
        ratio = starlette_time / axum_time

        assert ratio > 5, f"Expected Axum to be 5-10x faster, got {ratio}x"
```

#### 5.3: Documentation
```markdown
# docs/guides/choosing-http-server.md

## Choosing an HTTP Server

FraiseQL supports multiple HTTP server implementations. Choose based on your needs:

### Axum (Recommended for Production)
- **Performance**: 7-10x faster than Python alternatives
- **Concurrency**: Native async/await with tokio runtime
- **WebSocket**: Full support via Axum
- **When to use**: Performance-critical APIs, high-concurrency scenarios
- **Setup**: See `/docs/http-servers/axum.md`

### Starlette (Recommended for Python-first)
- **Performance**: Baseline Python async performance
- **Integration**: Works with existing Python ecosystem
- **Simplicity**: Pure Python, easy to understand and extend
- **When to use**: Python-heavy deployments, existing FastAPI codebases
- **Setup**: See `/docs/http-servers/starlette.md`

### FastAPI (Deprecated, for compatibility)
- **Status**: Maintenance mode, will be removed in v3.0
- **Performance**: Same as Starlette
- **Migration**: See `/docs/migration/fastapi-to-axum.md`
- **When to use**: Only if you have existing FastAPI code to migrate

## Performance Comparison

```
┌────────────────┬────────────┬──────────────┐
│ HTTP Server    │ Time/query │ Relative     │
├────────────────┼────────────┼──────────────┤
│ Axum (Rust)    │ 5ms        │ 1x (fastest) │
│ Starlette      │ 50ms       │ 10x slower   │
│ FastAPI        │ 55ms       │ 11x slower   │
└────────────────┴────────────┴──────────────┘
```

## Migration Path

```
Current (v1.9)          Future (v2.0)          End of Life (v3.0)
┌──────────────┐       ┌──────────────┐       ┌──────────────┐
│ FastAPI      │       │ FastAPI      │       │ FastAPI      │
│ (Primary)    │  →    │ (Deprecated) │  →    │ (Removed)    │
└──────────────┘       └──────────────┘       └──────────────┘
                       │ Axum         │
                       │ (Recommended)│
                       │ Starlette    │
                       │ (Alternative)│
```
```

**Deliverables**:
- [ ] Unified test suite with parity tests
- [ ] Performance benchmarks showing speed ratios
- [ ] `docs/guides/choosing-http-server.md` - Selection guide
- [ ] `docs/http-servers/` - Server-specific documentation
- [ ] `docs/migration/` - Migration guides
- [ ] All 5991+ tests pass across all servers
- [ ] Zero regressions

---

## 📊 File Structure After Implementation

```
fraiseql/
├── src/fraiseql/
│   ├── http/                           # NEW - Framework abstraction
│   │   ├── __init__.py
│   │   ├── interface.py                # HttpServer protocol
│   │   ├── context_builder.py          # Request parsing
│   │   ├── response_builder.py         # Response formatting
│   │   └── handlers/
│   │       ├── __init__.py
│   │       ├── graphql.py              # Main GraphQL handler
│   │       ├── health.py               # Health check
│   │       ├── introspection.py        # Introspection
│   │       └── subscriptions.py        # WebSocket subscriptions
│   │
│   ├── fastapi/                        # REFACTORED - Thin wrapper
│   │   ├── __init__.py
│   │   ├── app.py                      # FastAPI setup (delegates to http/)
│   │   ├── routers.py                  # FastAPI wrappers (calls http/handlers/)
│   │   └── middleware.py               # FastAPI middleware adapters
│   │
│   ├── starlette/                      # NEW - Python-native server
│   │   ├── __init__.py
│   │   ├── app.py                      # Starlette setup
│   │   ├── middleware.py               # Starlette middleware
│   │   └── handlers.py                 # Starlette route handlers
│   │
│   ├── axum/                           # NEW - Python bindings
│   │   ├── __init__.py
│   │   └── py_bindings.pyi             # Type stubs
│   │
│   └── ... (other modules unchanged)
│
├── fraiseql_rs/
│   ├── src/
│   │   ├── http/                       # NEW - Rust HTTP implementation
│   │   │   ├── mod.rs
│   │   │   ├── server.rs               # Axum app setup
│   │   │   ├── handlers.rs             # Route handlers
│   │   │   ├── middleware.rs           # Axum middleware
│   │   │   ├── context.rs              # Request context
│   │   │   └── response.rs             # Response building
│   │   │
│   │   └── py_bindings.rs              # PyO3 bindings
│   └── Cargo.toml                       # axum already listed
│
├── tests/
│   ├── integration/
│   │   ├── axum/                       # NEW
│   │   │   └── test_axum_server.py
│   │   ├── starlette/                  # NEW
│   │   │   └── test_starlette_server.py
│   │   └── test_all_http_servers.py    # NEW - Parity tests
│   │
│   ├── unit/
│   │   ├── http/                       # NEW
│   │   │   ├── test_http_interface.py
│   │   │   ├── test_context_builder.py
│   │   │   └── test_response_builder.py
│   │   └── ... (existing tests)
│   │
│   ├── benchmarks/
│   │   └── http_servers.py             # NEW - Performance comparison
│   │
│   └── ... (existing tests unchanged)
│
├── docs/
│   ├── http-servers/                   # NEW
│   │   ├── overview.md                 # Architecture overview
│   │   ├── axum.md                     # Axum setup & usage
│   │   ├── starlette.md                # Starlette setup & usage
│   │   └── fastapi.md                  # FastAPI (deprecated)
│   │
│   ├── guides/
│   │   ├── choosing-http-server.md     # NEW - Selection guide
│   │   └── ...
│   │
│   ├── migration/                      # NEW
│   │   ├── fastapi-to-axum.md          # Migration guide
│   │   └── fastapi-to-starlette.md     # Alternative migration
│   │
│   └── ... (existing docs)
│
├── .phases/
│   └── PLUGGABLE-HTTP-SERVERS.md       # This file
│
└── Makefile                            # New commands
    # make http-server-benchmarks
    # make test-axum
    # make test-starlette
    # make test-fastapi
```

---

## 🧪 Testing Strategy

### Test Layers

1. **Unit Tests** (`tests/unit/http/`)
   - Test HTTP abstraction interface
   - Context building from different frameworks
   - Response formatting consistency
   - Middleware execution order

2. **Integration Tests** (Per-server)
   - `tests/integration/axum/` - Axum-specific tests
   - `tests/integration/starlette/` - Starlette-specific tests
   - `tests/integration/fastapi/` - FastAPI compatibility tests

3. **Parity Tests** (`tests/integration/test_all_http_servers.py`)
   - All servers produce identical results
   - All servers format errors the same way
   - All servers handle APQ identically
   - All servers execute middleware in same order
   - All servers support subscriptions identically

4. **Performance Benchmarks**
   - Axum vs Starlette vs FastAPI
   - Prove 7-10x performance gain of Axum
   - Track performance regressions

### Test Execution
```bash
# Run all tests
make test

# Run per-server tests
make test-axum
make test-starlette
make test-fastapi

# Run parity tests only
make test-http-parity

# Run performance benchmarks
make test-benchmarks

# Quick feedback loop
make test-fast  # Only parity tests
```

---

## 🔄 Implementation Timeline

| Phase | Duration | Status | Dependencies |
|-------|----------|--------|--------------|
| **Phase 0** | Week 1 | Planning | None |
| **Phase 1** | Week 2-3 | Abstraction Layer | Phase 0 complete |
| **Phase 2** | Week 4-5 | Axum Server | Phase 1 complete |
| **Phase 3** | Week 6 | Starlette Server | Phase 1 complete |
| **Phase 4** | Week 7 | FastAPI Compat | Phase 2 or 3 complete |
| **Phase 5** | Week 8 | Testing & Docs | All phases complete |

**Total**: 8 weeks to complete

**Release**: v2.0.0 with Axum primary

---

## ✅ Acceptance Criteria

### Phase Completion
- [ ] All planned tests pass
- [ ] Zero regressions (5991+ tests still pass)
- [ ] Documented implementation approach
- [ ] Code review approval

### HTTP Server Parity
- [ ] All servers produce identical GraphQL results
- [ ] All servers return identical error formats
- [ ] All servers handle APQ caching identically
- [ ] All servers support subscriptions
- [ ] All servers execute middleware in same order

### Performance
- [ ] Axum achieves 7-10x speedup over Python servers
- [ ] Starlette and FastAPI have equivalent performance
- [ ] No regressions in existing query performance

### Documentation
- [ ] HTTP server architecture documented
- [ ] Selection guide for users
- [ ] Migration guides (FastAPI → Axum/Starlette)
- [ ] Server-specific setup instructions
- [ ] Performance comparison chart

### Deprecation
- [ ] FastAPI marked as deprecated in README
- [ ] Deprecation warnings in code
- [ ] Clear migration path documented
- [ ] Timeline for removal (v3.0)

---

## 🚀 Success Metrics

**Technical Metrics**:
- ✅ All 5991+ tests pass across all servers
- ✅ Zero regressions in existing functionality
- ✅ Axum performance: 7-10x improvement proven
- ✅ 100% test coverage for HTTP abstraction layer

**User-Facing Metrics**:
- ✅ Clear recommendation for new users (use Axum)
- ✅ Migration path for existing FastAPI users
- ✅ Documentation for all three servers
- ✅ Performance benchmarks published

**Architectural Metrics**:
- ✅ Single source of truth for business logic
- ✅ Framework-agnostic handlers (no duplication)
- ✅ Pluggable HTTP servers (add new one in days, not weeks)
- ✅ Zero coupling between HTTP layer and business logic

---

## 📝 Notes & Considerations

### Why Pluggable?
1. **Future frameworks**: Can add Quart, Litestar, or others quickly
2. **Use cases**: Some users want pure Python, others want peak performance
3. **Vendor lock-in**: Avoid strong ties to one framework
4. **Experimentation**: Can try new approaches without rewriting

### Why Axum Primary?
1. **Performance**: 7-10x faster than Python alternatives
2. **Modern**: Rust ecosystem is more active than Python web
3. **Resources**: Leverage Rust's superior concurrency story
4. **Ecosystem**: Axum integrates well with other Rust libraries

### Why Keep Starlette?
1. **Python teams**: Some prefer pure Python deployments
2. **Simplicity**: No compilation, faster iteration
3. **Ecosystem**: Works with existing Python middleware
4. **Choice**: Give users options

### Why Deprecate FastAPI?
1. **Overhead**: FastAPI adds little value over Starlette base
2. **Maintenance**: One less framework to test and debug
3. **Message**: Clear signal that Axum is the future
4. **Path**: Starlette provides migration path for Python users

---

## 🛠️ Developer Workflow

### Contributing a New Feature

**Example: Adding field-level authentication**

1. **Implement in abstraction layer** (`src/fraiseql/http/handlers/`)
   ```python
   async def apply_field_auth(context, field, value):
       """Framework-agnostic field authentication"""
       if context.user is None:
           return None  # Unauthorized
       if not context.user.can_read(field):
           return None  # Permission denied
       return value
   ```

2. **Add tests to parity suite**
   ```python
   async def test_field_auth(self, http_server):
       """All servers enforce field auth identically"""
       context = HttpContext(request_body=..., user=UserWithoutPermission())
       response = await http_server.handle_graphql(context)
       assert response.body["data"]["secret_field"] is None
   ```

3. **Test passes for all servers automatically**
   - No need to implement in Axum AND Starlette AND FastAPI
   - All inherit the behavior from abstraction layer

### Adding a New HTTP Server

**Example: Adding Quart support**

1. **Create `src/fraiseql/quart/app.py`**
   ```python
   async def create_quart_app(config):
       app = Quart(__name__)

       @app.route("/graphql", methods=["POST"])
       async def graphql():
           body = await request.get_json()
           context = HttpContext(request_body=body, ...)
           response = await execute_graphql_request(context, ...)
           return response.body, response.status_code

       return app
   ```

2. **Add tests to parity suite**
   ```python
   @pytest.fixture(params=["axum", "starlette", "fastapi", "quart"])
   async def http_server(self, request):
       # Automatically tested with all servers
   ```

3. **Done!** New server automatically gets all existing features

---

## 🎯 Success Criteria Checklist

### By End of Phase 0
- [ ] Architecture design document approved
- [ ] Decision on Axum/Starlette/FastAPI finalized
- [ ] Test strategy for parity documented
- [ ] Performance targets established

### By End of Phase 1
- [ ] HTTP server interface defined
- [ ] All existing business logic extracted
- [ ] Abstraction layer tests 100% passing
- [ ] Zero regressions in existing tests

### By End of Phase 2
- [ ] Axum server fully functional
- [ ] All Axum integration tests passing
- [ ] Axum performance benchmarks showing 7-10x improvement
- [ ] Axum documented and ready for production

### By End of Phase 3
- [ ] Starlette server fully functional
- [ ] All Starlette integration tests passing
- [ ] Parity tests proving identical behavior
- [ ] Starlette documented

### By End of Phase 4
- [ ] FastAPI refactored to thin wrapper
- [ ] Deprecation notices added
- [ ] Migration guides written
- [ ] FastAPI tests still passing

### By End of Phase 5
- [ ] All 5991+ tests passing across all servers
- [ ] Performance benchmarks published
- [ ] Documentation complete
- [ ] Release notes prepared for v2.0.0

---

## 📚 References & Related Documentation

- `.phases/PLUGGABLE-HTTP-SERVERS.md` - This file
- `docs/architecture/http-servers.md` - Architecture overview (to be created)
- `docs/http-servers/` - Server-specific guides (to be created)
- `docs/guides/choosing-http-server.md` - Selection guide (to be created)
- `docs/migration/` - Migration guides (to be created)

---

## 🔗 Dependencies & Blocked Items

### Blocking Implementation
- None—can start immediately with Phase 0

### Enables Future Work
- [x] Removing FastAPI entirely in v3.0
- [x] Adding new HTTP frameworks (Quart, Litestar, etc.)
- [x] Swapping HTTP servers without changing business logic
- [x] Testing business logic in isolation from HTTP layer

---

**Document Owner**: Architecture Team
**Last Updated**: January 5, 2026
**Status**: Ready for Review & Approval
