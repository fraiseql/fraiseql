# fraiseql-wire Development Guide

## Quick Start

### Local Development (without Docker)

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the project
cargo build

# Run unit tests
cargo test --lib

# Run clippy linter
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### Local Development (with Docker)

```bash
# Build Docker image
make docker-build

# Start PostgreSQL container
make docker-up

# Run unit tests
make test

# Run integration tests
make integration-test

# Stop container
make docker-down

# Clean up containers and volumes
make docker-clean
```

## Development Workflow

### Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

### Testing

```bash
# Run all unit tests
cargo test --lib

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run integration tests (requires Postgres on localhost:5432)
cargo test --test integration -- --ignored --nocapture
cargo test --test streaming_integration -- --ignored --nocapture
```

### Code Quality

```bash
# Format code
cargo fmt

# Check formatting without changes
cargo fmt -- --check

# Run linter
cargo clippy

# Run linter with strict warnings
cargo clippy -- -D warnings

# Run all checks
make check
```

### Documentation

```bash
# Build and open documentation
cargo doc --no-deps --open

# Check documentation builds without warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

## Docker Setup

### Using docker-compose

```bash
# Start PostgreSQL
docker-compose up -d

# View logs
docker-compose logs -f postgres

# Stop PostgreSQL
docker-compose down

# Remove volumes (clean slate)
docker-compose down -v
```

### Using Makefile

```bash
# Build image
make docker-build

# Start container
make docker-up

# View logs
make docker-logs

# Stop container
make docker-down

# Clean everything
make docker-clean
```

### Docker Connection Details

After starting the container:

- **Host**: `localhost` or `127.0.0.1`
- **Port**: `5432`
- **User**: `postgres`
- **Password**: `postgres`
- **Database**: `fraiseql_test`

### PostgreSQL Configuration

The Docker container is configured with:

- PostgreSQL 15 on Alpine Linux
- User `postgres` with password `postgres`
- Database `fraiseql_test` pre-created
- Listening on all interfaces (0.0.0.0)
- Health check enabled (5 retries, 2s interval)

## CI/CD Pipeline

The project uses GitHub Actions for continuous integration. The pipeline runs:

1. **Build & Unit Tests** (on every push and PR)
   - Builds the project
   - Runs unit tests
   - Runs clippy linter
   - Checks code formatting

2. **Integration Tests** (on every push and PR)
   - Runs integration tests against PostgreSQL service
   - Tests streaming functionality
   - Uses PostgreSQL 15 Alpine container

3. **Documentation** (on every push and PR)
   - Builds documentation
   - Checks for doc warnings

### Viewing CI/CD Status

- Push to repository to trigger workflow
- View results in GitHub Actions tab
- All checks must pass before merging PRs

## Project Structure

```
fraiseql-wire/
├── src/
│   ├── lib.rs                 # Library root
│   ├── error.rs               # Error types
│   ├── connection/            # Connection layer
│   ├── protocol/              # Wire protocol
│   ├── stream/                # Streaming abstractions
│   ├── json/                  # JSON validation
│   └── util/                  # Utilities
├── tests/
│   ├── integration.rs         # Connection integration tests
│   └── streaming_integration.rs # Streaming integration tests
├── examples/
│   └── basic_stream.rs        # Example application
├── Cargo.toml                 # Project manifest
├── Dockerfile                 # Docker image definition
├── docker-compose.yml         # Docker orchestration
├── Makefile                   # Development commands
├── .github/workflows/ci.yml   # GitHub Actions CI/CD
└── .claude/phases/            # Development phase plans
```

## Common Issues

### PostgreSQL Connection Refused

If you get "connection refused" when running integration tests:

1. Check PostgreSQL is running: `docker-compose ps`
2. Check connection: `docker-compose exec postgres pg_isready`
3. Wait longer: PostgreSQL can take 10+ seconds to start
4. Restart: `make docker-clean && make docker-up`

### Port Already in Use

If port 5432 is already in use:

1. Stop the container: `make docker-down`
2. Check what's using the port: `lsof -i :5432`
3. Or modify docker-compose.yml to use a different port

### Docker Build Failures

If Docker build fails:

1. Clean Docker cache: `docker system prune -a`
2. Rebuild: `make docker-build`
3. Check Docker is installed: `docker --version`
4. Check Docker daemon is running

## Performance Tips

### Build Time

- Use release builds for production: `cargo build --release`
- Cache dependencies: Use `--release` for first build
- Parallel build: `cargo build -j 4` (adjust for your CPU cores)

### Test Performance

- Run tests in parallel: `cargo test -- --test-threads=4`
- Run specific test: `cargo test test_name` (faster than all tests)
- Skip integration tests: `cargo test --lib` (unit tests only)

## Contributing

### Code Style

- Use `cargo fmt` to format code
- Follow Rust naming conventions (snake_case for functions/variables)
- Keep functions small and focused
- Use meaningful variable names

### Testing

- Write tests for new functionality
- Integration tests for public APIs
- Unit tests for internal functions
- Use `#[ignore]` for tests requiring external services

### Documentation

- Add doc comments to public items
- Use examples in doc comments
- Keep comments up-to-date with code
- Document assumptions and invariants

## Debugging

### Logging

Enable debug logging:

```bash
RUST_LOG=debug cargo test -- --nocapture
```

### Backtrace

Get detailed error traces:

```bash
RUST_BACKTRACE=full cargo test
```

### GDB Debugging

Debug with GDB:

```bash
cargo build
rust-gdb target/debug/fraiseql_wire
```

## Release Process

```bash
# Update version in Cargo.toml
vim Cargo.toml

# Run all checks
make check

# Build release
cargo build --release

# Tag release
git tag -a v0.2.0 -m "Release version 0.2.0"

# Push tag
git push origin v0.2.0
```

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Documentation](https://tokio.rs/)
- [PostgreSQL Protocol](https://www.postgresql.org/docs/current/protocol.html)
- [serde_json Documentation](https://docs.rs/serde_json/)
