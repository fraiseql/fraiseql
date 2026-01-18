# FraiseQL v2 - Deployment Guide

This guide covers deploying FraiseQL v2 GraphQL server in different environments.

## Table of Contents

1. [Local Development](#local-development)
2. [Docker](#docker)
3. [Kubernetes](#kubernetes)
4. [Configuration](#configuration)
5. [Database Setup](#database-setup)
6. [Monitoring](#monitoring)
7. [Troubleshooting](#troubleshooting)

---

## Local Development

### Prerequisites

- Rust 1.81+ ([install](https://rustup.rs/))
- PostgreSQL 14+ ([install](https://www.postgresql.org/download/))
- Git

### Setup

1. **Clone repository**

   ```bash
   git clone https://github.com/fraiseql/fraiseql-v2.git
   cd fraiseql-v2
   ```

2. **Create database**

   ```bash
   createdb fraiseql
   createuser fraiseql -P
   psql fraiseql -c "GRANT ALL PRIVILEGES ON DATABASE fraiseql TO fraiseql;"
   ```

3. **Compile schema**

   ```bash
   cargo run -p fraiseql-cli -- compile schemas/example.json -o schemas/schema.compiled.json
   ```

4. **Run server**

   ```bash
   # Set database URL
   export DATABASE_URL="postgresql://fraiseql:password@localhost:5432/fraiseql"

   # Run server
   cargo run -p fraiseql-server
   ```

5. **Test**

   ```bash
   curl -X POST http://localhost:8000/graphql \
     -H "Content-Type: application/json" \
     -d '{"query": "{ users { id name } }"}'
   ```

---

## Docker

### Build Image

```bash
# Build image
docker build -t fraiseql:latest .

# Or use provided image
docker pull fraiseql/fraiseql-server:latest
```

### Run Container

```bash
# Simple run
docker run -p 8000:8000 \
  -e DATABASE_URL="postgresql://user:pass@postgres:5432/fraiseql" \
  fraiseql:latest

# With volume mounts
docker run -p 8000:8000 \
  -v $(pwd)/schemas:/app/schemas:ro \
  -e DATABASE_URL="postgresql://user:pass@postgres:5432/fraiseql" \
  fraiseql:latest
```

### Docker Compose (Development)

```bash
# Start stack
docker-compose up -d

# View logs
docker-compose logs -f fraiseql-server

# Stop stack
docker-compose down

# Stop and remove volumes
docker-compose down -v
```

**Services included**:

- PostgreSQL (port 5432)
- FraiseQL Server (port 8000)
- Redis (optional, port 6379)

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | - | PostgreSQL connection string (required) |
| `RUST_LOG` | `info` | Logging level (debug, info, warn, error) |
| `FRAISEQL_BIND_ADDR` | `0.0.0.0:8000` | Server bind address |
| `FRAISEQL_SCHEMA_PATH` | `/app/schemas/schema.compiled.json` | Schema file path |
| `FRAISEQL_POOL_MIN_SIZE` | `5` | Database connection pool minimum size |
| `FRAISEQL_POOL_MAX_SIZE` | `20` | Database connection pool maximum size |
| `FRAISEQL_CORS_ENABLED` | `true` | Enable CORS |
| `FRAISEQL_COMPRESSION_ENABLED` | `true` | Enable response compression |

---

## Kubernetes

### Prerequisites

- Kubernetes 1.24+
- kubectl configured
- Docker image registry (e.g., Docker Hub)

### Deployment

1. **Create namespace**

   ```bash
   kubectl create namespace fraiseql
   ```

2. **Update secrets**

   ```bash
   kubectl -n fraiseql create secret generic fraiseql-secrets \
     --from-literal=database-url="postgresql://user:pass@postgres:5432/fraiseql"
   ```

3. **Create ConfigMap with schemas**

   ```bash
   kubectl -n fraiseql create configmap fraiseql-schemas \
     --from-file=schemas/schema.compiled.json
   ```

4. **Deploy server**

   ```bash
   kubectl apply -f k8s/service.yaml
   kubectl apply -f k8s/deployment.yaml
   ```

5. **Verify deployment**

   ```bash
   # Check pod status
   kubectl -n fraiseql get pods

   # View logs
   kubectl -n fraiseql logs -f deployment/fraiseql-server

   # Check service
   kubectl -n fraiseql get svc
   ```

### Accessing the Server

```bash
# Port forward (for local testing)
kubectl -n fraiseql port-forward svc/fraiseql-server 8000:80

# Or use LoadBalancer (if external IP available)
kubectl -n fraiseql get svc fraiseql-server
```

### Scaling

```bash
# Scale to 5 replicas
kubectl -n fraiseql scale deployment fraiseql-server --replicas=5

# Auto-scale based on CPU
kubectl -n fraiseql autoscale deployment fraiseql-server \
  --min=3 --max=10 --cpu-percent=70
```

---

## Configuration

### Database Connection

**PostgreSQL**:

```
postgresql://user:password@host:5432/database
```

**MySQL**:

```
mysql://user:password@host:3306/database
```

**SQLite** (development only):

```
sqlite:///path/to/database.db
```

### Connection Pool Settings

Adjust based on expected load:

```bash
# For small deployments
FRAISEQL_POOL_MIN_SIZE=2
FRAISEQL_POOL_MAX_SIZE=10

# For medium deployments
FRAISEQL_POOL_MIN_SIZE=5
FRAISEQL_POOL_MAX_SIZE=20

# For large deployments
FRAISEQL_POOL_MIN_SIZE=10
FRAISEQL_POOL_MAX_SIZE=50
```

---

## Database Setup

### Create User and Database

```sql
-- Create user
CREATE USER fraiseql WITH PASSWORD 'secure_password';

-- Create database
CREATE DATABASE fraiseql OWNER fraiseql;

-- Grant privileges
GRANT ALL PRIVILEGES ON DATABASE fraiseql TO fraiseql;
```

### Initialize Schema

```bash
# Compile schema
fraiseql-cli compile schemas/app.json -o schemas/schema.compiled.json

# Copy to server
cp schemas/schema.compiled.json /app/schemas/

# Verify
curl http://localhost:8000/introspection
```

---

## Monitoring

### Health Check

```bash
# Check server health
curl http://localhost:8000/health

# Example response
{
  "status": "healthy",
  "database": {
    "connected": true,
    "database_type": "PostgreSQL",
    "active_connections": 5,
    "idle_connections": 15
  },
  "version": "2.0.0-alpha.1"
}
```

### Logging

View logs with appropriate filter:

```bash
# Docker
docker-compose logs -f fraiseql-server

# Kubernetes
kubectl -n fraiseql logs -f deployment/fraiseql-server

# Local
RUST_LOG=debug cargo run -p fraiseql-server
```

---

## Troubleshooting

### Server won't start

**Check database connection**:

```bash
# Test connection
psql $DATABASE_URL -c "SELECT 1"
```

### Health check fails

```bash
# Check health endpoint
curl -v http://localhost:8000/health
```

### GraphQL queries failing

**Check schema**:

```bash
# View schema metadata
curl http://localhost:8000/introspection
```
