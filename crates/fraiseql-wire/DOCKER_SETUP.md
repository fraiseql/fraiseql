# Docker & CI/CD Setup

## Overview

fraiseql-wire now includes a complete Docker and CI/CD infrastructure for local development and continuous integration.

## Quick Start

### Option 1: Automatic Setup (Recommended)

```bash
# Run the setup script (requires Docker and Rust)
./scripts/dev-setup.sh
```

This will:

1. Check dependencies (Docker, docker-compose, Rust)
2. Build the Docker image
3. Start PostgreSQL container
4. Build the Rust project
5. Run unit tests
6. Attempt integration tests

### Option 2: Manual Setup with Makefile

```bash
# Build Docker image
make docker-build

# Start PostgreSQL
make docker-up

# Run tests
make test
make integration-test

# Stop containers
make docker-down
```

### Option 3: Manual Setup with docker-compose

```bash
# Start PostgreSQL
docker-compose up -d

# Wait for it to be ready
docker-compose exec -T postgres pg_isready -U postgres

# Run tests
cargo test --test integration -- --ignored --nocapture

# Stop containers
docker-compose down
```

## Docker Configuration

### Dockerfile

Alpine Linux-based PostgreSQL 15 container with:

- PostgreSQL 15 installed from Alpine packages
- Pre-initialized database with test data
- Startup script that waits for PostgreSQL readiness
- Health checks configured

**Build**: `docker-compose build` or `make docker-build`

### docker-compose.yml

Orchestrates PostgreSQL service:

- Service name: `postgres`
- Container name: `fraiseql-postgres`
- Port: `5433` (localhost) → `5432` (container)
- User: `postgres` / Password: `postgres`
- Database: `fraiseql_test`
- Health checks with retries
- Volume persistence

**Start**: `docker-compose up -d` or `make docker-up`

**Stop**: `docker-compose down` or `make docker-down`

### Environment Variables

When running tests against Docker PostgreSQL:

```bash
POSTGRES_HOST=localhost
POSTGRES_PORT=5433
POSTGRES_USER=postgres
POSTGRES_PASSWORD=postgres
POSTGRES_DB=fraiseql_test
```

Or use the default connection string in integration tests:

```rust
Transport::connect_tcp("localhost", 5433).await?
```

## Connection Details

After starting the container:

```
Host: localhost
Port: 5433
User: postgres
Password: postgres
Database: fraiseql_test
```

### Testing Connection

```bash
# Using docker-compose
docker-compose exec postgres psql -U postgres -d fraiseql_test

# Using psql locally (if installed)
psql -h localhost -p 5433 -U postgres -d fraiseql_test

# Using docker command
docker exec -it fraiseql-postgres psql -U postgres -d fraiseql_test
```

## Makefile Commands

```bash
make help              # Show all available commands
make build             # Build the project
make test              # Run unit tests
make integration-test  # Run integration tests
make clippy            # Run linter
make fmt               # Format code
make check             # Run all checks
make docker-build      # Build Docker image
make docker-up         # Start containers
make docker-down       # Stop containers
make docker-logs       # View PostgreSQL logs
make docker-clean      # Remove containers and volumes
```

## GitHub Actions CI/CD

The project includes automated CI/CD with GitHub Actions:

### Workflows

1. **Build & Test** (on every push/PR)
   - Builds project
   - Runs unit tests
   - Runs clippy linter
   - Checks code formatting

2. **Integration Tests** (on every push/PR)
   - Spins up PostgreSQL 15 service
   - Runs integration tests
   - Runs streaming tests

3. **Documentation** (on every push/PR)
   - Builds documentation
   - Checks for doc warnings

### Workflow File

Location: `.github/workflows/ci.yml`

Runs on:

- Push to `main` or `develop` branches
- Pull requests to `main` or `develop` branches

Services:

- PostgreSQL 15 (Alpine) automatically started for integration tests
- Caches dependencies for faster builds

## Development Script

The setup script (`scripts/dev-setup.sh`) provides one-command environment setup:

```bash
./scripts/dev-setup.sh
```

It verifies:

- Docker installation
- docker-compose installation
- Rust toolchain
- Then builds and tests everything

## Troubleshooting

### Port Already in Use

If port 5433 is in use:

1. Edit `docker-compose.yml`:

   ```yaml
   ports:
     - "127.0.0.1:5434:5432"  # Use 5434 instead
   ```

2. Update integration tests to use the new port:

   ```rust
   Transport::connect_tcp("localhost", 5434).await?
   ```

### PostgreSQL Won't Start

Check logs:

```bash
docker-compose logs postgres
```

Restart:

```bash
make docker-clean
make docker-up
```

### Connection Refused

Wait longer for PostgreSQL to start (up to 30 seconds):

```bash
docker-compose exec -T postgres pg_isready -U postgres
```

### Build Failures

Clean and rebuild:

```bash
make clean
docker-compose down -v
make docker-build
cargo build
```

## Performance Tips

### Faster Docker Builds

The Dockerfile is optimized for Alpine Linux:

- Small base image (~7MB)
- Minimal dependencies
- Fast startup time

### Faster Test Runs

```bash
# Run only unit tests (no integration tests)
cargo test --lib

# Run specific test
cargo test test_name

# Parallel test execution
cargo test -- --test-threads=4
```

## File Structure

```
fraiseql-wire/
├── Dockerfile              # PostgreSQL container definition
├── docker-compose.yml      # Service orchestration
├── .dockerignore           # Docker build context filter
├── .github/
│   └── workflows/
│       └── ci.yml          # GitHub Actions workflows
├── Makefile                # Development commands
├── scripts/
│   └── dev-setup.sh        # Automated setup script
└── DEVELOPMENT.md          # Developer guide
```

## Common Tasks

### Start Fresh PostgreSQL

```bash
make docker-clean
make docker-up
```

### View Logs

```bash
make docker-logs
# or
docker-compose logs -f postgres
```

### Connect to PostgreSQL

```bash
docker-compose exec postgres psql -U postgres -d fraiseql_test
```

### Run Tests with Output

```bash
cargo test -- --nocapture
```

### Monitor PostgreSQL Health

```bash
docker-compose ps
```

## Next Steps

1. Use `make docker-up` to start PostgreSQL
2. Run `make test` to run unit tests
3. Run `make integration-test` to test with real database
4. Push to GitHub to see CI/CD in action
5. Check GitHub Actions tab for workflow results

## Additional Resources

- [Docker Documentation](https://docs.docker.com/)
- [docker-compose Reference](https://docs.docker.com/compose/compose-file/)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [PostgreSQL Docker Image](https://hub.docker.com/_/postgres)
