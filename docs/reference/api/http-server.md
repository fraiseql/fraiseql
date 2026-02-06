<!-- Skip to main content -->
---

title: FraiseQL HTTP Server Guide
description: The FraiseQL HTTP server is a high-performance GraphQL execution engine built with Axum that loads pre-compiled schemas and executes GraphQL queries with zero r
keywords: ["directives", "types", "scalars", "schema", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL HTTP Server Guide

## Overview

The FraiseQL HTTP server is a high-performance GraphQL execution engine built with Axum that loads pre-compiled schemas and executes GraphQL queries with zero runtime parsing overhead.

## Architecture

```text
<!-- Code example in TEXT -->
Client Request
    ↓
HTTP Handler (Axum)
    ↓
Request Validation (depth, complexity, variables)
    ↓
GraphQL Execution (Executor)
    ↓
Database Adapter (PostgreSQL/MySQL/SQLite)
    ↓
Response Formatting (GraphQL spec-compliant)
    ↓
Client Response
```text
<!-- Code example in TEXT -->

## Getting Started

### Configuration

Configure the server via environment variables or the `ServerConfig` struct:

```rust
<!-- Code example in RUST -->
use fraiseql_server::{Server, ServerConfig};

let config = ServerConfig::new()
    .with_host("127.0.0.1")
    .with_port(8000)
    .with_database_url("postgresql://user:pass@localhost/db")
    .with_pool_size(20);

let server = Server::new(config).await?;
server.run().await?;
```text
<!-- Code example in TEXT -->

### Environment Variables

```bash
<!-- Code example in BASH -->
# Server
FRAISEQL_HOST=127.0.0.1              # Default: 127.0.0.1
FRAISEQL_PORT=8000                    # Default: 8000
FRAISEQL_SCHEMA_PATH=schema.compiled.json  # Required

# Database
DATABASE_URL=postgresql://user:pass@localhost/db  # Required

# Connection Pool
FRAISEQL_POOL_MIN=5                   # Default: 5
FRAISEQL_POOL_MAX=20                  # Default: 20
FRAISEQL_POOL_TIMEOUT_SECS=30         # Default: 30

# Query Validation
FRAISEQL_MAX_QUERY_DEPTH=10           # Default: 10
FRAISEQL_MAX_QUERY_COMPLEXITY=100     # Default: 100
```text
<!-- Code example in TEXT -->

## HTTP Endpoints

### GraphQL Query Endpoint

**POST** `/graphql`

Executes GraphQL queries with optional variables.

#### Request Format

```json
<!-- Code example in JSON -->
{
  "query": "query GetUser($id: ID!) { user(id: $id) { id name email } }",
  "variables": {
    "id": "123"
  },
  "operationName": "GetUser"
}
```text
<!-- Code example in TEXT -->

**Fields:**

- `query` (string, required): GraphQL query string
- `variables` (object, optional): Query variables as JSON object
- `operationName` (string, optional): Operation name for multi-operation queries

#### Response Format (Success)

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "123",
      "name": "John Doe",
      "email": "john@example.com"
    }
  }
}
```text
<!-- Code example in TEXT -->

#### Response Format (Error)

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Query exceeds maximum depth: 15 > 10",
      "code": "VALIDATION_ERROR",
      "locations": [
        {
          "line": 1,
          "column": 1
        }
      ],
      "path": ["user", "profile", "settings"]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Health Check Endpoint

**GET** `/health`

Returns server and database health status.

#### Response Format

```json
<!-- Code example in JSON -->
{
  "status": "healthy",
  "database": {
    "connected": true,
    "connection_pool": {
      "active": 3,
      "idle": 7,
      "max": 20
    }
  },
  "schema": {
    "loaded": true,
    "path": "schema.compiled.json"
  }
}
```text
<!-- Code example in TEXT -->

**Status Values:**

- `healthy`: All systems operational
- `degraded`: Database connected but slow
- `unhealthy`: Database connection failed or schema not loaded

### Schema Introspection Endpoint

**GET** `/introspection`

Returns GraphQL schema introspection data for schema discovery.

#### Response Format

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "User",
      "kind": "OBJECT",
      "description": "Represents a user",
      "fields": [
        {
          "name": "id",
          "type": "ID!",
          "description": "Unique identifier"
        },
        {
          "name": "name",
          "type": "String!",
          "description": "User's full name"
        },
        {
          "name": "email",
          "type": "String",
          "description": "User's email address"
        }
      ]
    }
  ]
}
```text
<!-- Code example in TEXT -->

## Query Validation

The server validates all incoming queries before execution to prevent abuse and resource exhaustion.

### Depth Validation

Prevents deeply nested queries that could cause exponential query complexity.

```graphql
<!-- Code example in GraphQL -->
# ✅ OK (depth = 3)
{
  user {
    profile {
      settings
    }
  }
}

# ❌ FAIL (depth = 5, exceeds default max of 10)
{
  user {
    posts {
      comments {
        author {
          profile {
            settings
          }
        }
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

**Default**: Maximum depth of 10 levels
**Configuration**: Set `FRAISEQL_MAX_QUERY_DEPTH` environment variable

### Complexity Validation

Scores query complexity based on structural patterns to prevent resource-exhausting queries.

**Scoring:**

- Each `{` = 1 point
- Each `[` = 2 points (array selections cost more)
- Each `(` = 1 point (arguments)

```graphql
<!-- Code example in GraphQL -->
# ✅ OK (complexity = 4)
{
  user {
    posts {
      title
    }
  }
}

# ❌ FAIL (complexity = 7, exceeds default max of 100)
{
  users [
    posts [
      comments [
        author
      ]
    ]
  ]
}
```text
<!-- Code example in TEXT -->

**Default**: Maximum complexity of 100 points
**Configuration**: Set `FRAISEQL_MAX_QUERY_COMPLEXITY` environment variable

### Variable Validation

Ensures query variables are properly formatted objects.

```json
<!-- Code example in JSON -->
// ✅ Valid
{
  "query": "query($id: ID!) { ... }",
  "variables": {
    "id": "123",
    "name": "John"
  }
}

// ❌ Invalid - variables must be object
{
  "query": "query($id: ID!) { ... }",
  "variables": ["123", "John"]
}
```text
<!-- Code example in TEXT -->

## Error Handling

The server implements GraphQL spec-compliant error responses with detailed error information for client-side handling.

### Error Codes

| Code | HTTP Status | Meaning |
|------|-------------|---------|
| `VALIDATION_ERROR` | 400 | Query validation failed (depth, complexity, syntax) |
| `PARSE_ERROR` | 400 | Malformed GraphQL query |
| `REQUEST_ERROR` | 400 | Invalid request format |
| `UNAUTHENTICATED` | 401 | Authentication required |
| `FORBIDDEN` | 403 | Access denied |
| `NOT_FOUND` | 404 | Requested resource not found |
| `CONFLICT` | 409 | Request conflicts with resource state |
| `DATABASE_ERROR` | 500 | Database operation failed |
| `INTERNAL_SERVER_ERROR` | 500 | Unexpected server error |
| `TIMEOUT` | 408 | Request timeout |
| `RATE_LIMIT_EXCEEDED` | 429 | Too many requests |

### Error Response Structure

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "User-friendly error message",
      "code": "ERROR_CODE",
      "locations": [
        {
          "line": 1,
          "column": 5
        }
      ],
      "path": ["field", "subfield"],
      "extensions": {
        "category": "VALIDATION",
        "status": 400,
        "request_id": "req-abc123"
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

**Fields:**

- `message`: Human-readable error description
- `code`: Machine-readable error code (enum)
- `locations`: Position in query where error occurred
- `path`: Field path that caused the error
- `extensions`: Additional context (optional)

### Common Error Scenarios

#### Query Too Deep

```text
<!-- Code example in TEXT -->
Query exceeds maximum depth of 10: depth = 15
```text
<!-- Code example in TEXT -->

**Solution**: Simplify query nesting or contact administrator to increase limit

#### Query Too Complex

```text
<!-- Code example in TEXT -->
Query exceeds maximum complexity of 100: score = 157
```text
<!-- Code example in TEXT -->

**Solution**: Reduce number of array selections or requested fields

#### Malformed Query

```text
<!-- Code example in TEXT -->
Empty query
```text
<!-- Code example in TEXT -->

**Solution**: Provide non-empty GraphQL query string

#### Database Error

```text
<!-- Code example in TEXT -->
Failed to connect to database: connection refused
```text
<!-- Code example in TEXT -->

**Solution**: Verify database URL and credentials in environment variables

## Performance Tuning

### Connection Pool Configuration

The connection pool manages database connections efficiently.

```bash
<!-- Code example in BASH -->
# Increase pool size for high-concurrency workloads
FRAISEQL_POOL_MIN=10
FRAISEQL_POOL_MAX=50

# Adjust timeout for slow network connections
FRAISEQL_POOL_TIMEOUT_SECS=60
```text
<!-- Code example in TEXT -->

**Recommendations:**

- `POOL_MIN`: Set to 5-10 for low-traffic (development) or 10-20 for production
- `POOL_MAX`: Set to 20-50 for typical workloads, up to 100 for high-concurrency
- `POOL_TIMEOUT`: Default 30 seconds is suitable for most cases; increase for slow networks

### Query Limits Configuration

Adjust validation limits based on your schema complexity.

```bash
<!-- Code example in BASH -->
# More permissive for complex schemas
FRAISEQL_MAX_QUERY_DEPTH=15
FRAISEQL_MAX_QUERY_COMPLEXITY=200

# Stricter for public APIs
FRAISEQL_MAX_QUERY_DEPTH=5
FRAISEQL_MAX_QUERY_COMPLEXITY=50
```text
<!-- Code example in TEXT -->

### Monitoring

Monitor key metrics to identify performance issues:

- **Health endpoint response time**: Should be <10ms
- **GraphQL query latency**: Typical 10-100ms depending on query complexity
- **Connection pool utilization**: Monitor `active` vs `max` connections
- **Error rate**: Track validation errors vs execution errors

## Deployment

### Docker

```dockerfile
<!-- Code example in DOCKERFILE -->
FROM rust:latest as builder
WORKDIR /app
COPY . .
RUN cargo build --release -p FraiseQL-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/FraiseQL-server /usr/local/bin/
COPY schema.compiled.json /app/schema.json
WORKDIR /app
ENV FRAISEQL_SCHEMA_PATH=/app/schema.json
EXPOSE 8000
CMD ["FraiseQL-server"]
```text
<!-- Code example in TEXT -->

### Environment Setup

```bash
<!-- Code example in BASH -->
# .env.production
FRAISEQL_HOST=0.0.0.0
FRAISEQL_PORT=8000
FRAISEQL_SCHEMA_PATH=/app/schema.compiled.json
DATABASE_URL=postgresql://prod_user:${DB_PASSWORD}@db.prod.internal/fraiseql_prod
FRAISEQL_POOL_MIN=10
FRAISEQL_POOL_MAX=50
FRAISEQL_POOL_TIMEOUT_SECS=30
FRAISEQL_MAX_QUERY_DEPTH=10
FRAISEQL_MAX_QUERY_COMPLEXITY=100
```text
<!-- Code example in TEXT -->

### Kubernetes

```yaml
<!-- Code example in YAML -->
apiVersion: apps/v1
kind: Deployment
metadata:
  name: FraiseQL-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: FraiseQL-server
  template:
    metadata:
      labels:
        app: FraiseQL-server
    spec:
      containers:
      - name: FraiseQL-server
        image: myregistry/FraiseQL-server:v2.0
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-credentials
              key: url
        - name: FRAISEQL_POOL_MAX
          value: "50"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
```text
<!-- Code example in TEXT -->

## Troubleshooting

### Server Won't Start

**Error**: `Failed to bind server: Address already in use`

**Solution**: Change port or kill existing process:

```bash
<!-- Code example in BASH -->
kill -9 $(lsof -t -i:8000)
FRAISEQL_PORT=8001 cargo run
```text
<!-- Code example in TEXT -->

### Database Connection Failed

**Error**: `Failed to connect to database: connection refused`

**Checklist**:

1. Verify DATABASE_URL is correct: `psql $DATABASE_URL -c "SELECT 1"`
2. Check database is running: `docker ps` or check service status
3. Verify credentials: username, password, host, port, database name
4. Check network connectivity: `ping db.host.com`
5. Review connection pool settings: may be too aggressive

### Query Timeout

**Error**: `Request timeout` (408)

**Solutions**:

1. Simplify query (reduce complexity)
2. Increase timeout: `FRAISEQL_POOL_TIMEOUT_SECS=60`
3. Optimize database indexes
4. Check database server load

### High Latency

**Error**: Queries taking >500ms

**Diagnostics**:

1. Run same query directly against database: `psql -c "EXPLAIN ANALYZE ..."`
2. Check connection pool utilization via `/health`
3. Monitor database CPU/memory
4. Review slow query logs
5. Add database indexes

### Memory Leak

**Error**: Memory usage grows over time

**Debugging**:

1. Check if connection pool is properly bounded (POOL_MAX)
2. Monitor open connections: `SELECT count(*) FROM pg_stat_activity`
3. Verify response body is being properly released
4. Check for circular references in compiled schema

## API Clients

### cURL

```bash
<!-- Code example in BASH -->
# Simple query
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'

# Query with variables
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query($id: ID!) { user(id: $id) { name } }",
    "variables": {"id": "123"}
  }'

# Health check
curl http://localhost:8000/health
```text
<!-- Code example in TEXT -->

### JavaScript/Node.js

```javascript
<!-- Code example in JAVASCRIPT -->
const response = await fetch('http://localhost:8000/graphql', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    query: 'query { users { id name } }'
  })
});

const data = await response.json();
if (data.errors) {
  console.error('GraphQL Error:', data.errors[0].message);
} else {
  console.log('Result:', data.data);
}
```text
<!-- Code example in TEXT -->

### Python

```python
<!-- Code example in Python -->
import requests

response = requests.post('http://localhost:8000/graphql', json={
    'query': 'query { users { id name } }'
})

data = response.json()
if 'errors' in data:
    print(f"Error: {data['errors'][0]['message']}")
else:
    print(f"Result: {data['data']}")
```text
<!-- Code example in TEXT -->

## Next Steps

- See [graphql-api.md](./graphql-api.md) for detailed GraphQL API specification
- See [Deployment Guide](../../deployment/guide.md) for production deployment
- See [examples/](../../../examples/) for example schemas and queries
