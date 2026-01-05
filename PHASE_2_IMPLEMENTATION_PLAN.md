# Phase 2: Python Wrapper for Axum HTTP Server

**Objective**: Create a Python wrapper that mirrors FastAPI's API while leveraging Axum's 7-10x performance.

**Status**: Planning Complete, Ready for Implementation

---

## Executive Summary

The Phase 1 Rust implementation (PyAxumServer) provides low-level FFI bindings to the Axum HTTP server. Phase 2 wraps this in a Python-friendly API that:

- ✅ Mirrors the FastAPI API (drop-in replacement)
- ✅ Hides Rust FFI details from users
- ✅ Adds framework features (middleware, config, lifecycle)
- ✅ Maintains identical DX (developer experience)
- ✅ Delivers 7-10x performance improvement

---

## Architecture

```
User Code (Python)
    ↓
create_axum_fraiseql_app()  ← Public API (Phase 2)
    ↓
AxumServer Wrapper Class    ← Framework layer
    ↓
PyAxumServer (Rust FFI)     ← Phase 1 binding
    ↓
Axum HTTP Server (Rust)     ← Phase 1 core
    ↓
GraphQL Pipeline (Rust)
    ↓
PostgreSQL
```

---

## Implementation Plan

### 1. Core Module Structure

**New files to create**:

```
src/fraiseql/
├── axum/                              # New module
│   ├── __init__.py                   # Package initialization + exports
│   ├── app.py                        # create_axum_fraiseql_app factory
│   ├── config.py                     # AxumFraiseQLConfig
│   ├── server.py                     # AxumServer wrapper class
│   ├── runtime.py                    # Tokio runtime management
│   ├── middleware.py                 # Middleware definitions
│   ├── responses.py                  # Response handling utilities
│   └── dependencies.py               # Dependency injection helpers
├── axum.pyi                          # Type stubs for IDE support
```

**Reuse from FastAPI**:
- Configuration patterns from `fastapi/config.py`
- Dependency injection from `fastapi/dependencies.py`
- Response handling from `fastapi/response_handlers.py`

---

### 2. Configuration Class: AxumFraiseQLConfig

**File**: `src/fraiseql/axum/config.py`

```python
class AxumFraiseQLConfig(BaseModel):
    """Configuration for Axum-based FraiseQL server.

    Drop-in replacement for FraiseQLConfig, with identical defaults.
    """

    # Database (from FastAPI)
    database_url: str
    database_pool_size: int = 10
    database_pool_timeout: int = 30
    database_max_overflow: int = 20

    # Environment
    environment: str = "development"
    production_mode: bool = False

    # GraphQL Features
    enable_introspection: bool = True
    enable_playground: bool = True
    playground_tool: str = "graphiql"
    max_query_depth: int = 10

    # Security
    auth_enabled: bool = False
    jwt_secret: str | None = None
    jwt_algorithm: str = "HS256"

    # Performance
    enable_query_caching: bool = False
    cache_ttl: int = 300

    # Error handling
    hide_error_details: bool = False

    # Axum HTTP Server (new)
    axum_host: str = "127.0.0.1"
    axum_port: int = 8000
    axum_workers: int = None  # Auto-detect from CPU count
    axum_metrics_token: str = ""

    # CORS (new - Axum config)
    cors_origins: list[str] | None = None
    cors_allow_credentials: bool = True
    cors_allow_methods: list[str] | None = None
    cors_allow_headers: list[str] | None = None

    # Response compression (new)
    enable_compression: bool = True
    compression_algorithm: str = "brotli"  # or "zstd"
    compression_min_bytes: int = 256
```

**Key Design Decisions**:

1. **Full backward compatibility** with FastAPI config (add Axum-specific fields)
2. **Sensible defaults** matching FastAPI
3. **Optional fields** for CORS and compression (use Axum defaults if not specified)
4. **Auto-detect workers** from CPU count (standard practice)

---

### 3. Factory Function: create_axum_fraiseql_app

**File**: `src/fraiseql/axum/app.py`

```python
def create_axum_fraiseql_app(
    *,
    config: AxumFraiseQLConfig | None = None,
    database_url: str | None = None,
    types: list[Type[Any]] | None = None,
    mutations: list[Type[Any]] | None = None,
    queries: list[Type[Any]] | None = None,
    subscriptions: list[Type[Any]] | None = None,
    context_getter: Callable[..., Coroutine[Any, Any, dict[str, Any]]] | None = None,
    middleware: list[Any] | None = None,
    cors_origins: list[str] | None = None,
    title: str = "FraiseQL API",
    description: str = "GraphQL API built with FraiseQL",
    version: str = "1.0.0",
    **kwargs: Any,
) -> AxumServer:
    """Create Axum-based FraiseQL server (7-10x faster than FastAPI).

    Drop-in replacement for create_fraiseql_app, with identical API.

    Args:
        config: Optional AxumFraiseQLConfig. If not provided, creates one from kwargs.
        database_url: PostgreSQL URL. Uses config if config provided.
        types: GraphQL types (@fraiseql.type decorated)
        mutations: GraphQL mutations
        queries: GraphQL queries
        subscriptions: GraphQL subscriptions
        context_getter: Async function to build request context
        middleware: Axum middleware instances
        cors_origins: CORS allowed origins
        title: API title (for docs)
        description: API description (for docs)
        version: API version
        **kwargs: Additional config parameters

    Returns:
        AxumServer instance (ready to call .start() or .run())

    Example:
        ```python
        from fraiseql.axum import create_axum_fraiseql_app

        # Create app (identical to FastAPI)
        app = create_axum_fraiseql_app(
            database_url="postgresql://localhost/db",
            types=[User, Post],
            mutations=[CreateUser],
        )

        # Run server
        app.start(host="0.0.0.0", port=8000)
        ```
    """

    # Build config from parameters
    if config is None:
        config = AxumFraiseQLConfig(
            database_url=database_url or kwargs.pop("database_url"),
            cors_origins=cors_origins or kwargs.get("cors_origins"),
            **kwargs
        )

    # Create Axum server wrapper
    server = AxumServer(config=config)

    # Register types, mutations, queries, subscriptions
    if types:
        server.register_types(types)
    if mutations:
        server.register_mutations(mutations)
    if queries:
        server.register_queries(queries)
    if subscriptions:
        server.register_subscriptions(subscriptions)

    # Initialize middleware
    if middleware:
        for m in middleware:
            server.add_middleware(m)

    return server
```

**Key Design Decisions**:

1. **Identical signature** to `create_fraiseql_app`
2. **Flexible config**: Create from object or kwargs
3. **Return AxumServer**: Can immediately call `.start()`
4. **All GraphQL features**: Types, mutations, queries, subscriptions

---

### 4. Server Wrapper: AxumServer Class

**File**: `src/fraiseql/axum/server.py`

```python
class AxumServer:
    """Axum-based GraphQL server wrapper.

    Provides Python-friendly interface to Rust Axum HTTP server.
    Wraps PyAxumServer FFI binding and adds lifecycle management.
    """

    def __init__(self, config: AxumFraiseQLConfig):
        """Initialize Axum server.

        Args:
            config: AxumFraiseQLConfig instance
        """
        self._config = config
        self._py_server: PyAxumServer | None = None
        self._runtime: TokioRuntime | None = None
        self._is_running = False
        self._types: dict[str, Type] = {}
        self._mutations: dict[str, Type] = {}
        self._queries: dict[str, Type] = {}
        self._subscriptions: dict[str, Type] = {}

    # ===== Type Registration =====

    def register_types(self, types: list[Type[Any]]) -> None:
        """Register GraphQL types."""
        for type_ in types:
            self._types[type_.__name__] = type_

    def register_mutations(self, mutations: list[Type[Any]]) -> None:
        """Register GraphQL mutations."""
        for mut in mutations:
            self._mutations[mut.__name__] = mut

    def register_queries(self, queries: list[Type[Any]]) -> None:
        """Register GraphQL queries."""
        for query in queries:
            self._queries[query.__name__] = query

    def register_subscriptions(self, subscriptions: list[Type[Any]]) -> None:
        """Register GraphQL subscriptions."""
        for sub in subscriptions:
            self._subscriptions[sub.__name__] = sub

    def add_middleware(self, middleware: Any) -> None:
        """Add Axum middleware."""
        # TODO: Implement after Phase 16
        pass

    # ===== Lifecycle Management =====

    def start(
        self,
        host: str | None = None,
        port: int | None = None,
        workers: int | None = None,
    ) -> None:
        """Start the HTTP server (blocking).

        Args:
            host: Bind address (default: from config)
            port: Bind port (default: from config)
            workers: Number of worker threads (default: from config)

        Raises:
            RuntimeError: If server is already running
            ValueError: If database connection fails

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")
            app.start(host="0.0.0.0", port=8000)  # Blocking call
            ```
        """
        if self._is_running:
            raise RuntimeError("Server is already running")

        host = host or self._config.axum_host
        port = port or self._config.axum_port
        workers = workers or self._config.axum_workers

        # Create Tokio runtime
        self._runtime = TokioRuntime(num_workers=workers)

        # Create PyAxumServer via FFI
        self._py_server = PyAxumServer.new(
            database_url=self._config.database_url,
            metrics_admin_token=self._config.axum_metrics_token,
        )

        # Start HTTP server in background thread
        self._runtime.spawn_server(
            py_server=self._py_server,
            host=host,
            port=port,
        )

        self._is_running = True

        # Log startup message
        logger.info(
            f"FraiseQL Axum server running at http://{host}:{port}",
            extra={
                "performance": "7-10x faster than FastAPI",
                "graphql": "/graphql",
                "metrics": "/metrics",
            }
        )

        # Keep main thread alive
        self._runtime.wait_for_shutdown()

    async def start_async(
        self,
        host: str | None = None,
        port: int | None = None,
    ) -> None:
        """Start server asynchronously (non-blocking).

        Args:
            host: Bind address
            port: Bind port

        Raises:
            RuntimeError: If server already running

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")
            await app.start_async(host="0.0.0.0", port=8000)
            # Server runs in background
            await asyncio.sleep(10)
            await app.shutdown()
            ```
        """
        if self._is_running:
            raise RuntimeError("Server is already running")

        host = host or self._config.axum_host
        port = port or self._config.axum_port

        self._py_server = PyAxumServer.new(
            database_url=self._config.database_url,
            metrics_admin_token=self._config.axum_metrics_token,
        )

        # Start server in background (async)
        self._py_server.start(host=host, port=port)
        self._is_running = True

        logger.info(f"FraiseQL Axum server started at http://{host}:{port}")

    async def shutdown(self) -> None:
        """Gracefully shutdown the server.

        Closes all connections and cleans up resources.

        Example:
            ```python
            await app.shutdown()
            ```
        """
        if not self._is_running:
            return

        if self._py_server:
            self._py_server.shutdown()

        if self._runtime:
            self._runtime.shutdown()

        self._is_running = False
        logger.info("FraiseQL Axum server stopped")

    def is_running(self) -> bool:
        """Check if server is running."""
        return self._is_running and (self._py_server and self._py_server.is_running())

    # ===== Request Execution (Direct API) =====

    async def execute_query(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Execute GraphQL query directly (without HTTP).

        Useful for:
        - Testing
        - Background jobs
        - Internal operations

        Args:
            query: GraphQL query string
            variables: Query variables
            operation_name: Name of operation (if multiple)
            context: Execution context

        Returns:
            GraphQL response: {"data": {...}} or {"errors": [...]}

        Example:
            ```python
            result = await app.execute_query(
                'query GetUser($id: ID!) { user(id: $id) { id name } }',
                variables={"id": "123"}
            )
            ```
        """
        if not self._py_server:
            raise RuntimeError("Server not initialized")

        import json

        variables_json = json.dumps(variables) if variables else None
        result_json = self._py_server.execute_query(
            query=query,
            variables=variables_json,
            operation_name=operation_name,
        )

        return json.loads(result_json)

    # ===== Configuration & Introspection =====

    def get_config(self) -> AxumFraiseQLConfig:
        """Get server configuration."""
        return self._config

    def get_schema(self) -> dict[str, Any]:
        """Get GraphQL schema introspection."""
        introspection_query = """
        query IntrospectionQuery {
            __schema {
                types { name description }
                queryType { name }
                mutationType { name }
                subscriptionType { name }
            }
        }
        """
        return asyncio.run(self.execute_query(introspection_query))

    async def get_metrics(self) -> dict[str, Any]:
        """Get server metrics (Prometheus format).

        Requires valid metrics_admin_token in config.
        """
        if not self._py_server:
            raise RuntimeError("Server not initialized")

        import json
        metrics_str = self._py_server.get_metrics()
        return {"metrics": metrics_str}

    # ===== Context Management =====

    @contextmanager
    def running(self, host: str = "127.0.0.1", port: int = 8000):
        """Context manager for server lifecycle.

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")

            with app.running(host="0.0.0.0", port=8000):
                # Server is running here
                response = requests.post(
                    "http://0.0.0.0:8000/graphql",
                    json={"query": "{ users { id } }"}
                )
                assert response.status_code == 200
            # Server is stopped here
            ```
        """
        self.start(host=host, port=port)
        try:
            yield self
        finally:
            if self._is_running:
                asyncio.run(self.shutdown())
```

**Key Design Decisions**:

1. **Dual API**: `start()` (blocking) and `start_async()` (non-blocking)
2. **Direct query execution**: `execute_query()` for tests/jobs
3. **Context manager**: `running()` for safe lifecycle
4. **Type registration**: Mirror GraphQL layer from FastAPI
5. **Metrics access**: Expose Rust metrics to Python

---

### 5. Runtime Management: TokioRuntime Class

**File**: `src/fraiseql/axum/runtime.py`

Manages the Tokio async runtime that runs the HTTP server:

```python
class TokioRuntime:
    """Tokio runtime wrapper for blocking server execution."""

    def __init__(self, num_workers: int | None = None):
        """Initialize Tokio runtime.

        Args:
            num_workers: Number of worker threads (default: CPU count)
        """
        import os
        num_workers = num_workers or os.cpu_count() or 4
        self._handle = tokio_new_runtime(num_workers)
        self._is_shutdown = False

    def spawn_server(
        self,
        py_server: PyAxumServer,
        host: str,
        port: int,
    ) -> None:
        """Spawn HTTP server in runtime."""
        # Implementation details depend on PyAxumServer.start()
        pass

    def wait_for_shutdown(self) -> None:
        """Block until shutdown signal."""
        # Keep main thread alive
        pass

    def shutdown(self) -> None:
        """Gracefully shutdown runtime."""
        # Send shutdown signal and clean up
        pass
```

**Note**: This depends on PyAxumServer.start() implementation (Phase 2 in Rust).

---

### 6. Dependency Injection Helpers

**File**: `src/fraiseql/axum/dependencies.py`

Reuse patterns from FastAPI:

```python
def get_db() -> Any:
    """Dependency injection for database connection."""
    # Get from request context
    pass

def get_current_user() -> Any:
    """Dependency injection for authenticated user."""
    # Extract from JWT token in request
    pass

async def get_auth_context() -> dict[str, Any]:
    """Build authentication context from request."""
    # Extract user/permissions from token
    pass
```

---

### 7. Type Stubs (.pyi File)

**File**: `src/fraiseql/axum.pyi`

```python
from collections.abc import Coroutine
from typing import Any, Callable, Type

class AxumFraiseQLConfig:
    database_url: str
    database_pool_size: int
    axum_host: str
    axum_port: int
    cors_origins: list[str] | None
    # ... more fields

    def __init__(self, *, database_url: str, **kwargs: Any) -> None: ...

class AxumServer:
    def __init__(self, config: AxumFraiseQLConfig) -> None: ...

    def start(
        self,
        host: str | None = None,
        port: int | None = None,
        workers: int | None = None,
    ) -> None: ...

    async def start_async(
        self,
        host: str | None = None,
        port: int | None = None,
    ) -> None: ...

    async def shutdown(self) -> None: ...

    def is_running(self) -> bool: ...

    async def execute_query(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
    ) -> dict[str, Any]: ...

def create_axum_fraiseql_app(
    *,
    config: AxumFraiseQLConfig | None = None,
    database_url: str | None = None,
    types: list[Type[Any]] | None = None,
    mutations: list[Type[Any]] | None = None,
    # ... more parameters
) -> AxumServer: ...
```

---

## Implementation Phases

### Phase 2A: Core Framework (Days 1-2)
- [ ] Create `src/fraiseql/axum/` module structure
- [ ] Implement `AxumFraiseQLConfig` class
- [ ] Implement `create_axum_fraiseql_app()` factory
- [ ] Implement `AxumServer` wrapper class
- [ ] Write type stubs (.pyi file)

### Phase 2B: Runtime & Lifecycle (Days 2-3)
- [ ] Implement `TokioRuntime` class
- [ ] Implement `start()` and `shutdown()` methods
- [ ] Implement background thread management
- [ ] Test lifecycle with PyAxumServer

### Phase 2C: Features & Polish (Days 3-4)
- [ ] Implement `execute_query()` method
- [ ] Add dependency injection helpers
- [ ] Add context manager support
- [ ] Add metrics endpoints
- [ ] Documentation and examples

### Phase 2D: Testing & QA (Days 4-5)
- [ ] Unit tests for configuration
- [ ] Unit tests for AxumServer wrapper
- [ ] Integration tests (full server lifecycle)
- [ ] Performance benchmarks (vs FastAPI)
- [ ] Fix all issues and edge cases

---

## API Compatibility Matrix

### Features Parity with FastAPI

| Feature | FastAPI | Axum | Status |
|---------|---------|------|--------|
| **Configuration** | FraiseQLConfig | AxumFraiseQLConfig | ✅ Compatible |
| **Factory Function** | create_fraiseql_app | create_axum_fraiseql_app | ✅ Identical signature |
| **Type Registration** | `.register_types()` | `.register_types()` | ✅ Same |
| **Mutation Registration** | `.register_mutations()` | `.register_mutations()` | ✅ Same |
| **Query Execution** | `execute_query()` | `execute_query()` | ✅ Same |
| **Server Start** | `.run()` | `.start()` | ⚠️ Different (Axum blocking) |
| **Async Start** | N/A | `.start_async()` | ✨ New (Axum feature) |
| **GraphQL Endpoint** | `/graphql` | `/graphql` | ✅ Same |
| **Introspection** | Available | Available | ✅ Same |
| **Error Handling** | FastAPI errors | GraphQL errors | ⚠️ Different HTTP semantics |
| **Middleware** | FastAPI middleware | Axum middleware | 🚧 Phase 16 |
| **Subscriptions** | FastAPI WebSocket | Axum WebSocket | ✅ Same protocol |
| **Metrics** | N/A | `/metrics` endpoint | ✨ New (Axum feature) |

---

## Testing Strategy

### Unit Tests
- `test_config.py`: Configuration validation
- `test_server.py`: Server initialization and state
- `test_factory.py`: Factory function behavior

### Integration Tests
- `test_full_lifecycle.py`: Start → Execute → Shutdown
- `test_query_execution.py`: Query execution through HTTP
- `test_error_handling.py`: Error responses
- `test_metrics.py`: Metrics endpoint

### Performance Tests
- Benchmark: Axum vs FastAPI (target: 7-10x)
- Benchmark: Query latency
- Benchmark: Concurrent requests

---

## Success Criteria

✅ **Core API**
- `create_axum_fraiseql_app()` works identically to FastAPI version
- `AxumServer.start()` blocks until shutdown
- `AxumServer.start_async()` allows non-blocking usage
- `execute_query()` executes GraphQL queries directly

✅ **Lifecycle**
- Server starts without errors
- HTTP requests are processed
- Server shuts down cleanly
- No resource leaks

✅ **Compatibility**
- Identical API to FastAPI version
- Same configuration options
- Same error handling
- Drop-in replacement

✅ **Performance**
- 7-10x faster than FastAPI (benchmarked)
- Sub-millisecond query latency
- Handles concurrent requests efficiently

✅ **Documentation**
- Examples for both `start()` and `start_async()`
- Migration guide from FastAPI
- Type stubs for IDE support
- Comprehensive docstrings

---

## Known Limitations & Phase 3+ Work

### Current Limitations (Phase 2)
- ⚠️ CORS configuration needs custom setup (uses Axum defaults)
- ⚠️ Custom middleware not supported (Phase 16)
- ⚠️ Subscriptions partial (Phase 15b)
- ⚠️ JWT auth stub (Phase 16, Commit 6)

### Phase 3+ Enhancements
- Custom CORS configuration
- Custom middleware support
- Full JWT authentication
- Advanced rate limiting
- Request tracing/observability
- Server health checks

---

## References

- **PyAxumServer FFI**: `fraiseql_rs/src/http/py_bindings.rs`
- **Axum Server**: `fraiseql_rs/src/http/axum_server.rs`
- **FastAPI Reference**: `src/fraiseql/fastapi/app.py`
- **FastAPI Config**: `src/fraiseql/fastapi/config.py`
- **Phase 1 Summary**: Rust HTTP server with GraphQL pipeline

---

## Checklist for Implementation

- [ ] Create directory structure
- [ ] Implement AxumFraiseQLConfig
- [ ] Implement create_axum_fraiseql_app()
- [ ] Implement AxumServer class
- [ ] Implement TokioRuntime wrapper
- [ ] Implement type stubs
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Benchmark vs FastAPI
- [ ] Write examples
- [ ] Update documentation
- [ ] Run full test suite
- [ ] Commit implementation

---

**Next Step**: Begin implementation with AxumFraiseQLConfig and factory function.
