# Starlette HTTP Server (Optional Framework Choice in v2.0.0)

**Version**: 2.0.0
**Status**: Production Ready
**Positioning**: NEW lightweight HTTP framework option

---

## Overview

Starlette is a NEW lightweight HTTP framework option added in v2.0.0.
It provides an alternative to FastAPI with identical GraphQL performance
through the same Rust pipeline.

## When to Use Starlette

- Starting a new FraiseQL project
- Wanting minimal HTTP dependencies
- Preferring Starlette's minimal API
- Migrating from FastAPI (optional, not required)

## When to Use FastAPI

- You have existing FastAPI code
- You need auto-generated API documentation
- Your team is familiar with FastAPI
- You want built-in validation and serialization

### Framework Architecture (v2.0.0)

**All Python frameworks now use the same high-performance Rust Axum HTTP server internally**:

| Python API | Internal Server | Performance | Use Case |
|------------|-----------------|-------------|----------|
| **Starlette** ⭐ | Rust Axum | 5-10x faster | New projects, modern Python |
| **FastAPI** | Rust Axum | 5-10x faster | Existing code, familiar API |
| **Direct Rust** | Native Axum | Maximum | Advanced users, peak performance |

**Key Insight**: The performance difference is negligible between Starlette/FastAPI - both use the same optimized Rust backend. Choose based on your Python preferences.

---

## Quick Start

### Installation

Starlette is already included as a dependency:

```bash
pip install fraiseql>=2.0.0
```

### Basic Example

```python
"""Minimal FraiseQL Starlette app."""

from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.starlette.app import create_starlette_app

# 1. Build schema from database
schema = build_fraiseql_schema(
    database_url="postgresql://user:password@localhost/mydb"
)

# 2. Create app
app = create_starlette_app(
    schema=schema,
    database_url="postgresql://user:password@localhost/mydb",
)

# 3. Run with uvicorn
# Command: uvicorn main:app --reload
```

Save as `main.py` and run:

```bash
pip install uvicorn
uvicorn main:app --reload
```

Visit:
- GraphQL: http://localhost:8000/graphql
- Health: http://localhost:8000/health

---

## Configuration

### Basic Configuration

```python
from fraiseql.starlette.app import create_starlette_app

app = create_starlette_app(
    schema=schema,
    database_url="postgresql://user:pass@localhost/db",
    cors_origins=["http://localhost:3000"],  # Allow frontend
    min_size=5,          # Connection pool: min connections
    max_size=20,         # Connection pool: max connections
    timeout=10,          # Connection: timeout in seconds
)
```

### Environment-Based Configuration

```python
"""Configuration from environment variables."""

import os
from fraiseql.starlette.app import create_starlette_app

DATABASE_URL = os.getenv(
    "DATABASE_URL",
    "postgresql://localhost/fraiseql_dev",
)

CORS_ORIGINS = os.getenv(
    "CORS_ORIGINS",
    "http://localhost:3000",
).split(",")

ENVIRONMENT = os.getenv("ENVIRONMENT", "development")

# Configure based on environment
pool_kwargs = {}
if ENVIRONMENT == "production":
    pool_kwargs["min_size"] = 10
    pool_kwargs["max_size"] = 50
else:
    pool_kwargs["min_size"] = 2
    pool_kwargs["max_size"] = 10

app = create_starlette_app(
    schema=schema,
    database_url=DATABASE_URL,
    cors_origins=CORS_ORIGINS,
    **pool_kwargs,
)
```

### With Authentication

```python
from fraiseql.auth.base import AuthProvider
from fraiseql.starlette.app import create_starlette_app

class MyAuthProvider(AuthProvider):
    async def authenticate(self, auth_header: str):
        """Extract and validate user from auth header."""
        if not auth_header.startswith("Bearer "):
            return None

        token = auth_header[7:]
        # Validate token...
        return {"user_id": "123", "roles": ["admin"]}

auth = MyAuthProvider()

app = create_starlette_app(
    schema=schema,
    database_url=database_url,
    auth_provider=auth,
)
```

### Production Configuration

```python
"""Production-ready Starlette setup."""

import logging
from fraiseql.starlette.app import create_starlette_app

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)

app = create_starlette_app(
    schema=schema,
    database_url=os.getenv("DATABASE_URL"),
    cors_origins=os.getenv("CORS_ORIGINS", "").split(","),
    min_size=10,
    max_size=50,
    timeout=30,
)

# Optional: Add request logging middleware
from starlette.middleware.base import BaseHTTPMiddleware

class RequestLoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        import time
        start = time.time()
        response = await call_next(request)
        duration = time.time() - start
        logging.info(
            f"{request.method} {request.url.path} "
            f"{response.status_code} {duration:.2f}s"
        )
        return response

app.add_middleware(RequestLoggingMiddleware)

# Run with:
# gunicorn main:app -w 4 -k uvicorn.workers.UvicornWorker
```

---

## API Endpoints

### GraphQL Query Endpoint

**POST** `/graphql`

Execute GraphQL queries and mutations.

#### Request Format

```json
{
  "query": "query { users { id name } }",
  "operationName": "GetUsers",
  "variables": {},
  "extensions": {}
}
```

#### Response Format

```json
{
  "data": {
    "users": [
      { "id": "1", "name": "Alice" },
      { "id": "2", "name": "Bob" }
    ]
  },
  "errors": null
}
```

#### Examples

```bash
# Simple query
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { users { id name } }"
  }'

# With variables
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query GetUser($id: ID!) { user(id: $id) { id name } }",
    "variables": { "id": "123" }
  }'

# With authentication
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-token-here" \
  -d '{"query": "query { me { id name } }"}'
```

### Health Check Endpoint

**GET** `/health`

Check server and database health.

#### Response (Healthy)

```json
{
  "status": "healthy",
  "version": "2.0.0",
  "database": "connected"
}
```

HTTP Status: **200**

#### Response (Unhealthy)

```json
{
  "status": "unhealthy",
  "version": "2.0.0",
  "error": "Database connection failed"
}
```

HTTP Status: **503**

#### Example

```bash
curl http://localhost:8000/health
```

### WebSocket Subscriptions (Optional)

**WebSocket** `/graphql/subscriptions`

Real-time subscriptions using the graphql-ws protocol.

#### Setup

```python
from fraiseql.starlette.subscriptions import add_subscription_routes

app = create_starlette_app(...)
add_subscription_routes(app, schema, db_pool)
```

---

## Features

### APQ (Automatic Persisted Queries)

Automatic query caching for improved performance:

```bash
# First request (query is cached)
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { users { id name } }",
    "extensions": {
      "persistedQuery": {
        "version": 1,
        "sha256Hash": "abc123..."
      }
    }
  }'

# Subsequent requests use hash only
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "extensions": {
      "persistedQuery": {
        "version": 1,
        "sha256Hash": "abc123..."
      }
    }
  }'
```

### Field Selection

Only request fields you need:

```graphql
# Request specific fields
query {
  users {
    id
    name
  }
}

# Skip extra fields for better performance
```

### Error Handling

Comprehensive error reporting:

```json
{
  "data": null,
  "errors": [
    {
      "message": "Invalid query: field 'xyz' not found",
      "extensions": {
        "code": "GRAPHQL_ERROR"
      }
    }
  ]
}
```

### Database Connection Pooling

Automatic connection pool management:

```python
app = create_starlette_app(
    schema=schema,
    database_url=database_url,
    min_size=5,          # Minimum connections
    max_size=20,         # Maximum connections
    timeout=10,          # Connection timeout
)
```

---

## Middleware & Customization

### Custom Request Processing

```python
from starlette.middleware.base import BaseHTTPMiddleware

class CustomMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        # Before request
        request.state.user_id = "123"

        response = await call_next(request)

        # After response
        response.headers["X-Custom"] = "value"
        return response

app.add_middleware(CustomMiddleware)
```

### Request Logging

```python
import logging
from starlette.middleware.base import BaseHTTPMiddleware

logger = logging.getLogger(__name__)

class LoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        import time
        start = time.time()
        response = await call_next(request)
        duration = time.time() - start

        logger.info(
            f"{request.method} {request.url.path} "
            f"{response.status_code} {duration:.2f}s"
        )
        return response

app.add_middleware(LoggingMiddleware)
```

### CORS Customization

```python
from starlette.middleware.cors import CORSMiddleware

app = create_starlette_app(...)

# More detailed CORS configuration
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000", "https://example.com"],
    allow_credentials=True,
    allow_methods=["GET", "POST", "OPTIONS"],
    allow_headers=["Content-Type", "Authorization"],
    expose_headers=["X-Custom-Header"],
    max_age=86400,
)
```

---

## Performance Optimization

### Connection Pool Tuning

```python
# For high-traffic applications
app = create_starlette_app(
    schema=schema,
    database_url=database_url,
    min_size=20,         # Keep connections warm
    max_size=100,        # Handle spikes
    timeout=30,          # Longer timeout for slow queries
)
```

### Deployment with Gunicorn

```bash
# Use multiple workers for better performance
gunicorn main:app \
  -w 4 \                          # 4 worker processes
  -k uvicorn.workers.UvicornWorker \
  --threads 2 \                   # 2 threads per worker
  --access-logfile - \
  --error-logfile -
```

### Docker Deployment

```dockerfile
FROM python:3.13-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install -r requirements.txt

# Copy application
COPY . .

# Run with gunicorn
CMD ["gunicorn", "main:app", \
  "-w", "4", \
  "-k", "uvicorn.workers.UvicornWorker", \
  "--bind", "0.0.0.0:8000"]
```

---

## Troubleshooting

### Database Connection Issues

```python
# Verify database connection at startup
import asyncio
from fraiseql.starlette.app import create_db_pool

async def test_connection():
    pool = await create_db_pool(database_url)
    try:
        async with pool.connection() as conn:
            result = await conn.execute("SELECT 1")
            print("✓ Database connection successful")
    except Exception as e:
        print(f"✗ Database connection failed: {e}")
    finally:
        await pool.close()

asyncio.run(test_connection())
```

### GraphQL Validation Errors

```bash
# Test query syntax
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { invalidField { id } }"
  }'

# Returns detailed error with line/column info
{
  "errors": [{
    "message": "Cannot query field 'invalidField' on type 'Query'",
    "locations": [{"line": 1, "column": 10}]
  }]
}
```

### Performance Issues

```python
# Enable query logging to identify slow queries
import logging

logging.getLogger("fraiseql").setLevel(logging.DEBUG)

# This will log:
# - Query parsing time
# - Database execution time
# - Total response time
```

---

## Migration from FastAPI

See [FastAPI Deprecation Plan](FASTAPI-DEPRECATION-PLAN.md) for detailed migration instructions.

**Quick Summary**: Replace imports and update app factory calls. Most code works unchanged.

```python
# OLD (FastAPI)
from fraiseql.fastapi import create_fraiseql_app
app = await create_fraiseql_app(schema, database_url)

# NEW (Starlette)
from fraiseql.starlette import create_starlette_app
app = create_starlette_app(schema, database_url)
```

---

## Comparison: Starlette vs Axum

| Feature | Starlette | Axum |
|---------|-----------|------|
| Language | Python | Rust |
| Setup Time | Instant | 15-30 min |
| Performance | Excellent | 5-10x faster |
| Async | Built-in | Built-in |
| Learning Curve | Low | Medium |
| Best For | Python users | Performance needs |

---

## Support & Resources

- **GitHub**: [fraiseql/fraiseql](https://github.com/fraiseql/fraiseql)
- **Issues**: [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)
- **Discussions**: [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- **Email**: support@fraiseql.dev

---

## What's Next?

1. **Choose Your Server**: Starlette (Python) or Axum (Performance)
2. **Deploy**: Follow deployment guides for your platform
3**Monitor**: Use health endpoints and logging
4. **Scale**: Adjust connection pool settings for your traffic

---

**Made with ❤️ by the FraiseQL Team**

Version: 2.0.0
Last Updated: January 5, 2026
