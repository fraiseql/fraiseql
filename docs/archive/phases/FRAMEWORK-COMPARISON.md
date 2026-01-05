# Framework Comparison Guide

## What is Pluggable HTTP Architecture?

v2.0.0 decouples HTTP servers from GraphQL execution via a protocol-based abstraction layer.
This means the same GraphQL schema runs identically on different HTTP frameworks.

**You choose the HTTP framework, not GraphQL performance:**

```
┌─────────────────────────────┐
│    Your GraphQL Schema      │
│    (unchanged on migration) │
└──────────────┬──────────────┘
               │
┌──────────────┴──────────────────────────────────┐
│  HTTP Framework Abstraction (v2.0.0)            │
│  RequestParser | ResponseFormatter | Middleware │
└──────────────┬──────────────────────────────────┘
               │
     ┌─────────┼─────────┬──────────────┐
     │         │         │              │
  Starlette FastAPI  Axum      (future frameworks)
     │         │         │              │
     └─────────┼─────────┴──────────────┘
               │
     ┌─────────▼──────────┐
     │ Rust GraphQL       │
     │ Pipeline           │
     │ (5-10x faster)     │
     └────────────────────┘
```

All paths deliver identical GraphQL results.

## Framework Options

### Starlette (NEW in v2.0.0)

**Summary**: Lightweight Python option, minimal dependencies

**Characteristics**:
- Minimal ASGI framework
- ~50MB on disk
- Fast startup (300ms)
- ~100MB memory baseline
- Pure Python/Cython

**Best For**:
- New FraiseQL projects
- Microservices needing lightweight footprint
- Teams valuing simplicity
- Docker images trying to minimize size

**Setup**:
```python
from fraiseql.starlette.app import create_starlette_app

app = create_starlette_app(
    schema=schema,
    database_url="postgresql://user:pass@localhost/db",
    cors_origins=["*"],
    auth_provider=auth,
    min_size=5,
    max_size=20,
    timeout=10,
)

# Auto routes: /graphql (POST), /health (GET)
```

**Config Options**:
```python
create_starlette_app(
    # Required
    schema: GraphQLSchema,
    database_url: str,

    # Optional
    auth_provider: AuthProvider | None = None,
    cors_origins: list[str] | None = None,
    enable_introspection: bool = True,

    # Pool config
    min_size: int = 5,
    max_size: int = 20,
    timeout: int = 10,
)
```

### FastAPI (Fully Supported in v2.0.0)

**Summary**: Feature-rich Python option, automatic API docs

**Characteristics**:
- Full-featured framework (validation, serialization, docs)
- ~200MB on disk
- Slower startup (500ms)
- ~120MB memory baseline
- Pydantic + Starlette

**Best For**:
- Existing FastAPI projects (no migration needed)
- Teams familiar with FastAPI patterns
- Projects using Pydantic models
- Auto-generated API documentation needed

**Setup**:
```python
from fraiseql.fastapi.app import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    debug=True,
    enable_introspection=True,
    turbo_mode=True,
    cors_origins=["*"],
)

app = create_fraiseql_app(
    schema=schema,
    db_pool=db_pool,
    config=config,
    auth=auth_provider,
)
```

**Config Options**:
```python
class FraiseQLConfig:
    debug: bool = True
    enable_introspection: bool = True
    turbo_mode: bool = False
    cors_origins: list[str] | None = None
    n_plus_one_detection: bool = False
    query_timeout: float = 30.0
```

### Axum (Native Rust Server)

**Summary**: Maximum performance native Rust option

**Characteristics**:
- Pure Rust, no Python runtime
- ~2MB binary
- Ultra-fast startup (<100ms)
- ~50MB memory baseline
- Direct Rust async I/O

**Best For**:
- High-traffic production deployments (5000+ req/sec)
- Teams wanting to eliminate Python overhead
- Maximum performance critical
- All-Rust tech stacks

**Setup**:
```rust
use fraiseql_rs::http::axum_server::{create_router, AppState};

#[tokio::main]
async fn main() {
    let pipeline = Arc::new(GraphQLPipeline::new(schema, db_pool));
    let metrics = Arc::new(HttpMetrics::new());

    let state = AppState::new(
        pipeline,
        metrics,
        "metrics_token".to_string(),
        None, // audit_logger
    );

    let router = create_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, router).await.unwrap();
}
```

**Performance Benefits**:
- Response time: 7-12ms uncached, <5ms cached (vs 12-22ms Python)
- Throughput: 5000+ req/sec (vs 1000 req/sec Python)
- Memory: <50MB (vs 100-150MB Python)
- Startup: <100ms (vs 300-500ms Python)
- No Python GIL contention

## Comparison Matrix

### Performance

| Metric | Starlette | FastAPI | Axum |
|--------|-----------|---------|------|
| **GraphQL execution** | 12-22ms | 12-22ms | 12-22ms |
| HTTP overhead | 2-3ms | 3-5ms | <1ms |
| **Total uncached** | ~15-25ms | ~15-27ms | ~13-23ms |
| **Total cached** | ~3-5ms | ~3-5ms | ~3-5ms |
| Startup time | 300ms | 500ms | 100ms |
| Memory baseline | 100MB | 120MB | 50MB |
| Disk footprint | 50MB | 200MB | 2MB |
| Max throughput | 2000 req/sec | 1500 req/sec | 5000+ req/sec |

### Features

| Feature | Starlette | FastAPI | Axum |
|---------|-----------|---------|------|
| Auto API docs | ❌ | ✅ | ❌ |
| Pydantic validation | ❌ | ✅ | ❌ |
| Dependency injection | Basic | Advanced | ❌ |
| Middleware ecosystem | Limited | Rich | Basic |
| WebSocket support | ✅ | ✅ | ✅ |
| Multipart uploads | ✅ | ✅ | ✅ |
| Authentication | ✅ | ✅ | ✅ |
| Authorization | ✅ | ✅ | ✅ |
| Rate limiting | ✅ | ✅ | ✅ |
| CSRF protection | ✅ | ✅ | ✅ |
| Audit logging | ✅ | ✅ | ✅ |
| OpenTelemetry | ✅ | ✅ | ✅ |
| Health checks | ✅ | ✅ | ✅ |
| Metrics | ✅ | ✅ | ✅ |
| Chaos testing | ✅ | ✅ | ✅ |
| APQ support | ✅ | ✅ | ✅ |
| GraphQL subscriptions | ✅ | ✅ | ✅ |

### Migration Effort

| From | To Starlette | To FastAPI | To Axum |
|------|--------------|------------|---------|
| **Existing FastAPI** | 30 min (optional) | N/A (already using) | 2-4 hours (Rust rewrite) |
| **New project** | 5 min (recommended) | 5 min | 15 min (Rust setup) |
| **Existing Starlette** | N/A | 10 min | 2-4 hours (Rust rewrite) |

## Migration Guide

### From FastAPI to Starlette (30 minutes)

1. Change imports:
```python
# Before
from fraiseql.fastapi.app import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig

# After
from fraiseql.starlette.app import create_starlette_app
```

2. Simplify config:
```python
# Before
config = FraiseQLConfig(
    debug=True,
    enable_introspection=True,
    turbo_mode=True,
    cors_origins=["*"],
)
app = create_fraiseql_app(schema, db_pool, config=config)

# After
app = create_starlette_app(
    schema=schema,
    database_url="postgresql://user:pass@localhost/db",
    cors_origins=["*"],
    enable_introspection=True,
)
```

3. Update pool configuration:
```python
# Add pool config to create_starlette_app call
app = create_starlette_app(
    # ... existing config
    min_size=5,
    max_size=20,
    timeout=10,
)
```

### From Starlette to Axum (2-4 hours)

1. Rewrite Python app to Rust
2. Use fraiseql_rs crates instead of fraiseql Python
3. Set up Rust async runtime
4. Configure Axum middleware
5. Deploy as native binary

See [Axum Native Server Guide](AXUM-NATIVE-SERVER.md) for detailed migration steps.

## Decision Tree

```mermaid
graph TD
    A[New FraiseQL Project?] -->|Yes| B[Want minimal dependencies?]
    A -->|No| C[Existing FastAPI project?]

    B -->|Yes| D[Use Starlette]
    B -->|No| E[Need auto-docs/validation?]
    E -->|Yes| F[Use FastAPI]
    E -->|No| G[Want maximum performance?]
    G -->|Yes| H[Use Axum]
    G -->|No| I[Use Starlette or FastAPI]

    C -->|Yes| J[Stay with FastAPI]
    C -->|No| K[Want to minimize Python?]
    K -->|Yes| L[Use Axum]
    K -->|No| M[Use Starlette]
```

## Conclusion

**All frameworks are equally valid choices.** The differences are in HTTP server overhead and ecosystem preferences, not GraphQL performance.

- **Choose Starlette** for minimal, modern Python
- **Choose FastAPI** for existing projects or rich features
- **Choose Axum** for maximum performance and Rust-native deployments

The pluggable architecture ensures your GraphQL schema works identically across all options.
