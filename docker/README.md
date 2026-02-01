# FraiseQL Docker Setup Guide

This directory contains Docker configuration for running FraiseQL, with support for both development/testing and newcomer onboarding scenarios.

## Quick Start (Newcomers)

For the fastest way to try FraiseQL without local Rust compilation:

```bash
docker compose -f docker-compose.demo.yml up -d
```

Then open:
- **GraphQL IDE**: http://localhost:3000
- **Tutorial**: http://localhost:3001
- **FraiseQL Server**: http://localhost:8000

See [../docs/docker-quickstart.md](../docs/docker-quickstart.md) for detailed instructions.

## Docker Files

### `docker-compose.demo.yml`

**Purpose**: Newcomer onboarding platform
**Services**:
- PostgreSQL 16 (blog database)
- FraiseQL Server (GraphQL API)
- Apollo Sandbox (GraphQL IDE)
- Tutorial Server (Interactive learning)

**Usage**:
```bash
# Start all services
docker compose -f docker-compose.demo.yml up -d

# View logs
docker compose -f docker-compose.demo.yml logs -f

# Stop all services
docker compose -f docker-compose.demo.yml down

# Remove data volumes (fresh start)
docker compose -f docker-compose.demo.yml down -v
```

### `../docker-compose.yml`

**Purpose**: Development and integration testing
**Services**:
- PostgreSQL 16 (primary test database)
- MySQL 8.0 (multi-database testing)
- SQL Server 2022 (enterprise testing)
- Optional: Redis, NATS (with profiles)

**Usage**:
```bash
# Start core databases
docker compose up -d

# Start with server
docker compose --profile with-server up -d

# Start everything
docker compose --profile with-server --profile with-redis --profile with-nats up -d
```

### `../docker-compose.test.yml`

**Purpose**: Comprehensive testing with all integrations
**Services**:
- PostgreSQL 16 with pgvector extension
- MySQL 8.3
- SQL Server 2022
- Redis 7
- NATS 2.10 with JetStream
- ClickHouse
- Elasticsearch 8.15

**Usage**:
```bash
# Start all test services
docker compose -f docker-compose.test.yml up -d

# Run integration tests
make test-integration
```

## Using Make Commands

The recommended way to manage Docker services:

```bash
# Demo (Newcomers)
make demo-start      # Start demo stack
make demo-stop       # Stop demo stack
make demo-logs       # View demo logs
make demo-status     # Check health
make demo-restart    # Restart services
make demo-clean      # Remove volumes and stop

# Development (Developers)
make db-up           # Start test databases
make db-down         # Stop test databases
make db-logs         # View database logs
make db-status       # Check database health
make db-reset        # Reset with fresh volumes
```

## Dockerfile

**Location**: `../Dockerfile`

**Build**: Multi-stage build producing optimized runtime image
- **Stage 1 (Builder)**: Rust 1.84-slim, compiles binaries
- **Stage 2 (Runtime)**: Debian bookworm-slim, minimal dependencies

**Binaries**: `fraiseql-server`, `fraiseql-cli`

**Build locally**:
```bash
docker build -t fraiseql:latest .
```

## Port Mapping

| Service | Demo Port | Dev Port | Test Port | Purpose |
|---------|-----------|----------|-----------|---------|
| FraiseQL Server | 8000 | 8000 | 8000 | GraphQL API |
| GraphQL IDE | 3000 | - | - | Query explorer |
| Tutorial | 3001 | - | - | Learning platform |
| PostgreSQL | 5432 | 5433 | 5433 | Primary database |
| MySQL | - | 3307 | 3307 | Secondary database |
| SQL Server | - | 1434 | 1434 | Enterprise database |
| Redis | - | 6379 | 6380 | Caching |
| NATS | - | 4223 | 4223 | Message broker |

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

- `FRAISEQL_API_URL`: FraiseQL server URL (for tutorial queries)
- `TUTORIAL_PORT`: Tutorial server port (default: 3001)
- `NODE_ENV`: Node environment (default: `production`)

## Troubleshooting

### Port Already in Use

If you get "Address already in use" errors:

```bash
# Find what's using the port
lsof -i :8000

# Kill the process
kill -9 <PID>

# Or change the port in docker-compose.demo.yml
```

### Service Won't Start

Check logs:
```bash
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

Clear browser cache or try incognito window. The IDE needs JavaScript enabled.

## Docker Network

All demo services run on the `fraiseql-demo` network, allowing inter-service communication by hostname:
- `postgres-blog` - PostgreSQL from server/tutorial
- `fraiseql-server` - GraphQL API from tutorial
- `tutorial` - Tutorial server

## Volume Management

### Persistent Data

Volumes are preserved between restarts:

```bash
# List volumes
docker volume ls | grep fraiseql

# Inspect volume
docker volume inspect fraiseql-postgres-blog-data
```

### Remove Volumes (Fresh Start)

```bash
# Demo stack
docker compose -f docker-compose.demo.yml down -v

# Dev/test stacks
docker compose down -v
docker compose -f docker-compose.test.yml down -v
```

## Production Considerations

These Docker setups are for **development and learning only**. For production:

1. **Security**
   - Change default passwords
   - Use secrets management (Docker Secrets, Kubernetes Secrets)
   - Enable TLS/SSL
   - Implement authentication

2. **Scalability**
   - Use Kubernetes or orchestration platform
   - Implement load balancing
   - Configure connection pooling

3. **Monitoring**
   - Add logging (ELK, Datadog, etc.)
   - Implement health checks
   - Track metrics

See [../docs/deployment/guide.md](../docs/deployment/guide.md) for production deployment.

## Building and Publishing Images

For CI/CD pipelines:

```bash
# Build for Docker Hub
docker build -t myrepo/fraiseql:latest .

# Push to registry
docker push myrepo/fraiseql:latest

# Tag by version
docker build -t myrepo/fraiseql:v2.0.0 .
docker push myrepo/fraiseql:v2.0.0
```

## Next Steps

- **Get Started**: [../docs/docker-quickstart.md](../docs/docker-quickstart.md)
- **Full Guide**: [../docs/GETTING_STARTED.md](../docs/GETTING_STARTED.md)
- **Examples**: [../examples/README.md](../examples/README.md)
- **Deployment**: [../docs/deployment/guide.md](../docs/deployment/guide.md)

## Support

- **Issues**: https://github.com/anthropics/fraiseql/issues
- **Discussions**: https://github.com/anthropics/fraiseql/discussions
- **Documentation**: https://github.com/anthropics/fraiseql/tree/main/docs
