# HTTP Server Integration Guide

Complete guide to integrating FraiseQL with HTTP servers for production GraphQL APIs.

## Overview

FraiseQL supports multiple HTTP server integrations, each optimized for different use cases and performance requirements. All Python frameworks use the same high-performance Rust Axum HTTP server internally, delivering 5-10x better performance than traditional Python servers.

**Available Servers:**

| Server | Language | Performance | Use Case | Best For |
|--------|----------|-------------|----------|----------|
| **Axum** | Rust | Maximum | Peak performance | Production, scale |
| **Starlette** | Python | High | Modern async Python | New projects |
| **FastAPI** | Python | High | Enterprise features | Existing FastAPI code |
| **Custom** | Any | Variable | Special requirements | Custom integrations |

## Quick Start

### Choose Your Server

**For new projects:**
```python
# Starlette (recommended for new projects)
from starlette.applications import Starlette
from fraiseql.starlette import FraiseQLApp

app = Starlette()
fraiseql_app = FraiseQLApp(schema=your_schema)
app.mount("/graphql", fraiseql_app)
```

**For existing FastAPI projects:**
```python
# FastAPI (recommended for existing projects)
from fastapi import FastAPI
from fraiseql.fastapi import FraiseQLRouter

app = FastAPI()
app.include_router(FraiseQLRouter(schema=your_schema))
```

**For maximum performance:**
```rust
// Axum (recommended for peak performance)
use axum::{Router, routing::post};
use fraiseql_axum::FraiseQLLayer;

let app = Router::new()
    .route("/graphql", post(graphql_handler))
    .layer(FraiseQLLayer::new(schema));
```

## Server Architecture

### Unified Performance Pipeline

All Python frameworks share the same optimized Rust HTTP pipeline:

```
Client Request
     ↓
Python Framework (FastAPI/Starlette)
     ↓
Rust Axum HTTP Server (shared)
     ↓
FraiseQL GraphQL Engine
     ↓
PostgreSQL Database
```

**Performance Characteristics:**
- **Throughput**: 10,000+ requests/second
- **Latency**: <5ms for cached queries
- **Memory**: <100MB baseline
- **Concurrency**: Async-first design

### Server Comparison

#### Axum (Rust) - Maximum Performance

```rust
use axum::{Router, routing::post, extract::Json};
use fraiseql_axum::{FraiseQLRequest, FraiseQLResponse};
use serde_json::Value;

async fn graphql_handler(
    Json(request): Json<FraiseQLRequest>
) -> Json<FraiseQLResponse> {
    let response = fraiseql_axum::execute(&schema, request).await;
    Json(response)
}

let app = Router::new()
    .route("/graphql", post(graphql_handler));
```

**Pros:**
- ✅ Maximum performance (native Rust)
- ✅ Lowest memory usage
- ✅ Zero-copy request handling
- ✅ Advanced async patterns

**Cons:**
- ❌ Rust learning curve
- ❌ Ecosystem maturity
- ❌ Deployment complexity

#### Starlette - Modern Python

```python
from starlette.applications import Starlette
from starlette.responses import JSONResponse
from starlette.routing import Route
from fraiseql.starlette import FraiseQLApp

async def graphql_handler(request):
    fraiseql_app = request.app.state.fraiseql
    return await fraiseql_app.handle_graphql(request)

routes = [
    Route("/graphql", graphql_handler, methods=["GET", "POST"]),
]

app = Starlette(routes=routes)
app.state.fraiseql = FraiseQLApp(schema=your_schema)
```

**Pros:**
- ✅ Modern async Python
- ✅ Minimal dependencies
- ✅ Fast startup time
- ✅ Easy testing

**Cons:**
- ❌ Less ecosystem than FastAPI
- ❌ Manual request handling

#### FastAPI - Enterprise Features

```python
from fastapi import FastAPI, Request, Response
from fraiseql.fastapi import FraiseQLRouter, get_graphql_context

app = FastAPI()

# Add GraphQL router
app.include_router(
    FraiseQLRouter(
        schema=your_schema,
        context_getter=get_graphql_context,
        graphiql=True  # Enable GraphiQL
    ),
    prefix="/graphql"
)

# Custom context function
def get_graphql_context(request: Request) -> dict:
    return {
        "request": request,
        "user": request.state.user,
        "db": request.app.state.db
    }
```

**Pros:**
- ✅ Auto-generated API docs
- ✅ Built-in validation
- ✅ Large ecosystem
- ✅ Enterprise features

**Cons:**
- ❌ Higher memory usage
- ❌ More dependencies

## Configuration

### Basic Configuration

```python
from fraiseql import create_app

app = create_app(
    schema=your_schema,
    server="starlette",  # or "fastapi" or "axum"
    database_url="postgresql://user:pass@localhost/db",
    enable_graphiql=True,
    enable_playground=False,
    cors_origins=["https://yourdomain.com"],
    max_complexity=1000,
    timeout_seconds=30
)
```

### Advanced Configuration

```python
from fraiseql.config import ServerConfig

config = ServerConfig(
    # Server settings
    host="0.0.0.0",
    port=8000,
    workers=4,

    # GraphQL settings
    max_depth=10,
    max_complexity=1000,
    timeout=30,

    # Security
    cors_origins=["*"],
    trusted_hosts=["yourdomain.com"],

    # Performance
    connection_pool_size=20,
    cache_size_mb=100,

    # Observability
    enable_metrics=True,
    enable_tracing=True,
    log_level="INFO"
)

app = create_app(schema=your_schema, config=config)
```

## Authentication & Authorization

### JWT Authentication

```python
from fraiseql.auth import JWTAuthProvider
from fraiseql.middleware import AuthMiddleware

auth_provider = JWTAuthProvider(
    secret_key="your-secret-key",
    algorithm="HS256"
)

app = create_app(
    schema=your_schema,
    auth_provider=auth_provider,
    middleware=[AuthMiddleware(auth_provider)]
)
```

### Custom Authentication

```python
from fraiseql.auth import AuthProvider, UserContext

class CustomAuthProvider(AuthProvider):
    async def validate_token(self, token: str) -> dict:
        # Custom token validation
        return decode_custom_token(token)

    async def get_user_context(self, token_data: dict) -> UserContext:
        return UserContext(
            user_id=token_data["user_id"],
            roles=token_data.get("roles", []),
            tenant_id=token_data.get("tenant_id")
        )

auth_provider = CustomAuthProvider()
app = create_app(schema=your_schema, auth_provider=auth_provider)
```

## Middleware

### Custom Middleware

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response

class CustomMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        # Pre-processing
        print(f"Request: {request.method} {request.url}")

        # Call next middleware/app
        response = await call_next(request)

        # Post-processing
        response.headers["X-Custom-Header"] = "value"

        return response

app = create_app(
    schema=your_schema,
    middleware=[CustomMiddleware()]
)
```

### CORS Middleware

```python
from starlette.middleware.cors import CORSMiddleware

app = create_app(
    schema=your_schema,
    middleware=[
        CORSMiddleware(
            allow_origins=["https://yourdomain.com"],
            allow_credentials=True,
            allow_methods=["*"],
            allow_headers=["*"],
        )
    ]
)
```

## Error Handling

### Global Error Handler

```python
from starlette.exceptions import ExceptionMiddleware
from starlette.responses import JSONResponse

async def graphql_error_handler(request, exc):
    return JSONResponse(
        {"error": "Internal server error"},
        status_code=500
    )

app = create_app(schema=your_schema)
app.add_exception_handler(Exception, graphql_error_handler)
```

### GraphQL-Specific Errors

```python
from graphql import GraphQLError

@fraiseql.mutation
async def dangerous_operation(info) -> str:
    try:
        result = await perform_dangerous_operation()
        return result
    except ValueError as e:
        raise GraphQLError(f"Operation failed: {str(e)}")
    except Exception as e:
        # Log internal error
        logger.error(f"Unexpected error: {e}")
        # Return user-friendly message
        raise GraphQLError("An unexpected error occurred")
```

## Performance Tuning

### Connection Pooling

```python
from fraiseql.db import DatabasePool

pool = DatabasePool(
    dsn="postgresql://user:pass@localhost/db",
    min_size=5,
    max_size=20,
    max_idle_time=300,
    max_lifetime=3600
)

app = create_app(
    schema=your_schema,
    database_pool=pool
)
```

### Caching

```python
from fraiseql.caching import PostgresCache, ResultCache

cache = ResultCache(
    backend=PostgresCache(connection_pool=pool),
    default_ttl=300,
    max_size_mb=500
)

app = create_app(
    schema=your_schema,
    cache=cache
)
```

### Monitoring

```python
from fraiseql.monitoring import PrometheusMetrics, HealthCheck

# Add Prometheus metrics
metrics = PrometheusMetrics()
app = create_app(schema=your_schema, metrics=metrics)

# Add health check endpoint
health = HealthCheck(
    checks=[
        lambda: pool.is_healthy(),
        lambda: cache.is_healthy()
    ]
)

@app.get("/health")
async def health_check():
    return await health.run_checks()
```

## Deployment

### Docker Deployment

```dockerfile
FROM python:3.13-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install -r requirements.txt

COPY . .

EXPOSE 8000

CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
    spec:
      containers:
      - name: fraiseql
        image: your-registry/fraiseql:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secret
              key: url
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
```

### Production Checklist

- [ ] Configure proper logging
- [ ] Set up monitoring and alerting
- [ ] Configure database connection pooling
- [ ] Enable caching for performance
- [ ] Set up proper CORS policies
- [ ] Configure authentication/authorization
- [ ] Enable HTTPS/TLS
- [ ] Set up rate limiting
- [ ] Configure backup and recovery
- [ ] Test failover scenarios

## Migration Guides

### From FastAPI to Starlette

```python
# Before (FastAPI)
from fastapi import FastAPI
from fraiseql.fastapi import FraiseQLRouter

app = FastAPI()
app.include_router(FraiseQLRouter(schema=schema))

# After (Starlette)
from starlette.applications import Starlette
from fraiseql.starlette import FraiseQLApp

app = Starlette()
fraiseql_app = FraiseQLApp(schema=schema)
app.mount("/graphql", fraiseql_app)
```

### From Express.js to FraiseQL

```javascript
// Before (Express.js)
const express = require('express');
const { graphqlHTTP } = require('express-graphql');

const app = express();
app.use('/graphql', graphqlHTTP({
  schema: schema,
  graphiql: true
}));

// After (FraiseQL)
from starlette.applications import Starlette
from fraiseql.starlette import FraiseQLApp

app = Starlette()
fraiseql_app = FraiseQLApp(schema=schema, enable_graphiql=True)
app.mount("/graphql", fraiseql_app)
```

## Troubleshooting

### Common Issues

**Connection Pool Exhaustion:**
```python
# Increase pool size
pool = DatabasePool(max_size=50, min_size=10)
```

**Slow Queries:**
```python
# Add database indexes
# Enable query logging
# Use EXPLAIN ANALYZE to identify bottlenecks
```

**Memory Leaks:**
```python
# Monitor memory usage
# Check for circular references
# Use memory profiling tools
```

**Rate Limiting:**
```python
from slowapi import Limiter
from slowapi.util import get_remote_address

limiter = Limiter(key_func=get_remote_address)

app = create_app(
    schema=your_schema,
    middleware=[limiter]
)
```

## Advanced Topics

### WebSocket Subscriptions

```python
from starlette.routing import WebSocketRoute
from fraiseql.subscriptions import SubscriptionHandler

async def websocket_handler(websocket):
    handler = SubscriptionHandler(schema=schema)
    await handler.handle(websocket)

routes = [
    Route("/graphql", graphql_handler, methods=["GET", "POST"]),
    WebSocketRoute("/graphql", websocket_handler),
]
```

### File Uploads

```python
from starlette.requests import Request
from fraiseql.uploads import UploadHandler

@fraiseql.mutation
async def upload_file(info, file: Upload) -> FileResult:
    handler = UploadHandler()
    result = await handler.save_upload(file)
    return result
```

### Custom Scalars

```python
from graphql import GraphQLScalarType
from fraiseql.scalars import DateTimeScalar

# Add custom scalars to schema
schema = make_executable_schema(
    type_defs,
    resolvers,
    scalars={
        "DateTime": DateTimeScalar,
        "UUID": UUIDScalar,
    }
)
```

## Next Steps

- [API Reference](../api/index.md) - Complete API documentation
- [Performance Guide](../guides/performance-guide.md) - Advanced optimization
- [Architecture Overview](../architecture/README.md) - System design
- [Monitoring](../production/monitoring.md) - Production monitoring

---

**All servers provide identical GraphQL performance through the shared Rust pipeline. Choose based on your team's preferences and existing infrastructure.**
