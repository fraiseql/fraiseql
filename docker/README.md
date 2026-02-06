# FraiseQL Docker Guide

Docker configuration for running FraiseQL with zero Rust compilation. Get a fully working GraphQL server with tutorial and admin dashboard in under a minute.

---

## Quick Start (30 Seconds)

### Option 1: Demo (Recommended for First-Time Users)

```bash
docker compose -f docker-compose.demo.yml up -d
```

Open your browser:

- **GraphQL IDE**: http://localhost:3000 (Query explorer)
- **Tutorial**: http://localhost:3001 (6-chapter interactive learning)
- **Server**: http://localhost:8000 (GraphQL API)

Stop with:

```bash
make demo-stop
```

### Option 2: Production Pre-built Images

For production deployments with minimal overhead:

```bash
# Single blog example
docker compose -f docker-compose.prod.yml up -d

# Or all 3 examples (blog, e-commerce, streaming)
docker compose -f docker-compose.prod-examples.yml up -d
```

Services run on:

- **Blog IDE**: http://localhost:3000
- **E-Commerce IDE**: http://localhost:3100 (if all examples)
- **Streaming IDE**: http://localhost:3200 (if all examples)
- **Tutorial**: http://localhost:3001
- **Admin Dashboard**: http://localhost:3002

### Option 3: Using Make (Easiest)

```bash
# Demo stack
make demo-start       # Start everything
make demo-status      # Check health
make demo-logs        # View logs
make demo-stop        # Stop everything
make demo-clean       # Fresh start (removes data)

# Production stack
make prod-start       # Single example
make prod-examples-start  # All examples
make prod-examples-status # Check health
```

---

## Docker Compose Files

### `docker-compose.demo.yml` - Newcomer Onboarding

**Best for**: Learning, experimentation, first-time users

**Services**:

- PostgreSQL 16 (blog database)
- FraiseQL Server (GraphQL API)
- Apollo Sandbox (GraphQL IDE)
- Tutorial Server (Interactive learning)
- Admin Dashboard (Debugging & monitoring)

**Usage**:

```bash
docker compose -f docker-compose.demo.yml up -d      # Start
docker compose -f docker-compose.demo.yml logs -f    # View logs
docker compose -f docker-compose.demo.yml down -v    # Stop (fresh start)
```

### `docker-compose.prod.yml` - Production Single Example

**Best for**: Production deployments, minimal resource footprint

**Services**:

- PostgreSQL 16
- FraiseQL Server
- Pre-built blog example

### `docker-compose.prod-examples.yml` - Production Multiple Examples

**Best for**: Showcasing capabilities, testing different scenarios

**Services**:

- PostgreSQL 16 (multi-example database)
- 3 FraiseQL instances (blog, e-commerce, streaming)
- Pre-built examples with sample data

### `docker-compose.yml` - Development & Testing

**Best for**: Developers, running tests, integration work

**Services**:

- PostgreSQL 16 (primary)
- MySQL 8.0 (multi-database testing)
- SQL Server 2022 (enterprise testing)
- Optional: Redis, NATS (use `--profile` flags)

**Usage**:

```bash
docker compose up -d                    # Core databases only
docker compose --profile with-server up -d  # Add FraiseQL server
docker compose --profile with-redis --profile with-nats up -d  # Everything
```

### `docker-compose.test.yml` - Comprehensive Testing

**Best for**: Running integration tests with all services

**Services**:

- PostgreSQL 16 with pgvector
- MySQL 8.3
- SQL Server 2022
- Redis 7
- NATS 2.10 with JetStream
- ClickHouse
- Elasticsearch 8.15

**Usage**:

```bash
docker compose -f docker-compose.test.yml up -d
make test-integration
```

---

## Make Commands

Recommended way to manage services:

```bash
# Demo Stack (Newcomers)
make demo-start        # Start demo
make demo-stop         # Stop demo
make demo-logs         # View logs
make demo-status       # Health check
make demo-restart      # Restart services
make demo-clean        # Reset with fresh data

# Production Stack (Single Example)
make prod-start        # Start single example
make prod-stop         # Stop
make prod-logs         # View logs
make prod-status       # Health check

# Production Stack (Multiple Examples)
make prod-examples-start   # Start all examples
make prod-examples-stop    # Stop all
make prod-examples-logs    # View logs
make prod-examples-status  # Health check

# Development Databases
make db-up             # Start test databases
make db-down           # Stop
make db-logs           # View logs
make db-status         # Health check
make db-reset          # Fresh start

# Help
make help              # All available commands
make help | grep demo  # Demo commands only
```

---

## Dockerfile

**Location**: `../Dockerfile`

**Type**: Multi-stage build

**Stages**:

1. **Builder**: Rust 1.84-slim - compiles `fraiseql-server` and `fraiseql-cli`
2. **Runtime**: Debian bookworm-slim - minimal dependencies, optimized image

**Build locally**:

```bash
docker build -t fraiseql:latest .
docker build -t fraiseql:v2.0.0 .
```

**Push to registry**:

```bash
docker tag fraiseql:latest myregistry/fraiseql:latest
docker push myregistry/fraiseql:latest
```

---

## Port Reference

| Service | Demo | Prod | Dev | Test | Purpose |
|---------|------|------|-----|------|---------|
| FraiseQL Server | 8000 | 8000 | 8000 | 8000 | GraphQL API |
| GraphQL IDE (Blog) | 3000 | 3000 | - | - | Query explorer |
| GraphQL IDE (E-Commerce) | - | 3100 | - | - | Advanced queries |
| GraphQL IDE (Streaming) | - | 3200 | - | - | Real-time data |
| Tutorial Server | 3001 | 3001 | - | - | Interactive learning |
| Admin Dashboard | 3002 | 3002 | - | - | Debugging & monitoring |
| PostgreSQL | 5432 | 5432 | 5433 | 5433 | Primary database |
| MySQL | - | - | 3307 | 3307 | Secondary database |
| SQL Server | - | - | 1434 | 1434 | Enterprise database |
| Redis | - | - | 6379 | 6380 | Caching |
| NATS | - | - | 4223 | 4223 | Message broker |

---

## Environment Variables

### FraiseQL Server

- `DATABASE_URL`: PostgreSQL connection string
- `FRAISEQL_SCHEMA_PATH`: Path to compiled schema JSON
- `FRAISEQL_BIND_ADDR`: Server bind address (default: `0.0.0.0:8000`)
- `RUST_LOG`: Log level (default: `info`)

### PostgreSQL

- `POSTGRES_DB`: Database name
- `POSTGRES_USER`: Username
- `POSTGRES_PASSWORD`: Password
- `POSTGRES_INITDB_ARGS`: Server initialization args

### Tutorial Server

- `FRAISEQL_API_URL`: FraiseQL server URL
- `TUTORIAL_PORT`: Tutorial server port (default: `3001`)
- `NODE_ENV`: Node environment (default: `production`)

---

## Troubleshooting

### Port Already in Use

```bash
# Find what's using the port
lsof -i :8000

# Kill the process
kill -9 <PID>

# Or modify docker-compose.demo.yml and change the port
```

### Services Won't Start

Check logs:

```bash
docker compose -f docker-compose.demo.yml logs
docker compose -f docker-compose.demo.yml logs fraiseql-server
```

### Database Connection Failed

Verify PostgreSQL is running:

```bash
docker compose -f docker-compose.demo.yml exec postgres-blog pg_isready
```

Reset the database:

```bash
docker compose -f docker-compose.demo.yml down -v
docker compose -f docker-compose.demo.yml up -d
```

### Tutorial Can't Connect to Server

Verify network connectivity:

```bash
docker compose -f docker-compose.demo.yml exec tutorial \
  curl -v http://fraiseql-server:8000/health
```

### GraphQL IDE Shows Blank Page

- Clear browser cache
- Try incognito/private window
- Ensure JavaScript is enabled
- Check that port 3000 is actually serving content

---

## Docker Network

Demo services run on the `fraiseql-demo` network for inter-service communication:

```
┌──────────────────┐
│  postgres-blog   │ (PostgreSQL database)
└────────┬─────────┘
         │ (connection string)
         ↓
┌──────────────────────────────────────────────────┐
│ fraiseql-server (GraphQL execution engine)       │
└────────┬───────────────────────────┬──────┬──────┘
         │                           │      │
         ↓                           ↓      ↓
    tutorial              apollo-sandbox admin-dashboard
   (learning)             (IDE)           (monitoring)
```

Service hostnames (from within containers):

- `postgres-blog` - PostgreSQL
- `fraiseql-server` - GraphQL API
- `tutorial` - Tutorial server

---

## Volume Management

### Persistent Data

Volumes persist between restarts:

```bash
# List all FraiseQL volumes
docker volume ls | grep fraiseql

# Inspect a specific volume
docker volume inspect fraiseql-postgres-blog-data
```

### Fresh Start (Remove Data)

```bash
# Demo stack
docker compose -f docker-compose.demo.yml down -v

# Dev/test stacks
docker compose down -v
docker compose -f docker-compose.test.yml down -v
```

---

## Production Deployment

These Docker setups are for **development and learning**. For production:

### Security

- Change all default passwords
- Use secrets management (Docker Secrets, Kubernetes Secrets, HashiCorp Vault)
- Enable TLS/SSL for all connections
- Implement authentication/authorization
- Run in isolated networks

### Scalability

- Use Kubernetes or Docker Swarm
- Implement load balancing (nginx, HAProxy)
- Configure connection pooling
- Use managed databases (RDS, Azure Database, etc.)
- Implement horizontal scaling for stateless services

### Monitoring

- Add centralized logging (ELK stack, Datadog, CloudWatch)
- Implement health checks and alerting
- Track metrics (Prometheus, New Relic)
- Set up distributed tracing (Jaeger)
- Monitor resource usage

See [../docs/deployment/guide.md](../docs/deployment/guide.md) for comprehensive production deployment guide.

---

## Building and Publishing Images

For CI/CD pipelines:

```bash
# Build for Docker Hub
docker build -t myregistry/fraiseql:latest .

# Tag by version
docker build -t myregistry/fraiseql:v2.0.0 .

# Push to registry
docker push myregistry/fraiseql:latest
docker push myregistry/fraiseql:v2.0.0
```

---

## Next Steps

- **Getting Started**: [../docs/GETTING_STARTED.md](../docs/GETTING_STARTED.md)
- **Docker Quick Start**: [../docs/docker-quickstart.md](../docs/docker-quickstart.md)
- **Full Documentation**: [../docs/README.md](../docs/README.md)
- **Examples**: [../examples/README.md](../examples/README.md)
- **Production Deployment**: [../docs/deployment/guide.md](../docs/deployment/guide.md)

---

## Support

- **Issues**: https://github.com/anthropics/fraiseql/issues
- **Discussions**: https://github.com/anthropics/fraiseql/discussions
- **Documentation**: https://github.com/anthropics/fraiseql/tree/main/docs
