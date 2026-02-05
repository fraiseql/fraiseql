# FraiseQL Testing Guide

This guide explains how to run integration tests with the Docker-based test infrastructure.

---

## Quick Start

### 1. Start Test Databases

```bash
# Start all test databases
docker compose -f docker-compose.test.yml up -d

# Wait for databases to be healthy (takes ~10-15 seconds)
docker compose -f docker-compose.test.yml ps

# Check logs if needed
docker compose -f docker-compose.test.yml logs -f postgres-test
```

### 2. Run Integration Tests

```bash
# Run all integration tests (includes ignored tests)
cargo test -- --ignored

# Run only fraiseql-core integration tests
cargo test -p fraiseql-core -- --ignored

# Run specific test
cargo test -p fraiseql-core test_postgres_adapter_creation -- --ignored --nocapture
```

### 3. Stop Test Databases

```bash
# Stop and remove containers (keeps data volumes)
docker compose -f docker-compose.test.yml down

# Stop and remove everything including data
docker compose -f docker-compose.test.yml down -v
```

---

## Test Infrastructure

### PostgreSQL Test Database (Primary)

- **Container:** `fraiseql-postgres-test`
- **Port:** `5433` (to avoid conflicts with local PostgreSQL on 5432)
- **Connection String:** `postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql`
- **Database:** `test_fraiseql`
- **User:** `fraiseql_test`
- **Password:** `fraiseql_test_password`

**Test Views:**

- `v_user` - 5 test users with email, name, age, role, tags, metadata
- `v_post` - 4 test posts with title, content, author (joined), published, views, tags
- `v_product` - 4 test products with name, price, stock, category, attributes

**Usage in tests:**

```rust
#[tokio::test]
#[ignore]
async fn test_postgres_query() {
    let adapter = PostgresAdapter::new(
        "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
    ).await.unwrap();

    let results = adapter.execute_where_query("v_user", None, None, None).await.unwrap();
    assert_eq!(results.len(), 5);
}
```

### PostgreSQL + pgvector Test Database

- **Container:** `fraiseql-postgres-vector-test`
- **Port:** `5434`
- **Connection String:** `postgresql://fraiseql_test:fraiseql_test_password@localhost:5434/test_fraiseql_vector`
- **Extensions:** `vector`, `uuid-ossp`

**Test Views:**

- `v_embedding` - 5 test embeddings with 3D vectors
- `v_document` - 4 test documents with full-text search vectors

**Usage in tests:**

```rust
#[tokio::test]
#[ignore]
async fn test_vector_operators() {
    let adapter = PostgresAdapter::new(
        "postgresql://fraiseql_test:fraiseql_test_password@localhost:5434/test_fraiseql_vector"
    ).await.unwrap();

    // Test cosine distance operator
    let where_clause = WhereClause::Field {
        path: vec!["embedding".to_string()],
        operator: WhereOperator::CosineDistance,
        value: json!("[1.0, 0.0, 0.0]"),
    };

    let results = adapter.execute_where_query("v_embedding", Some(&where_clause), None, None).await.unwrap();
}
```

### MySQL Test Database (Secondary)

- **Container:** `fraiseql-mysql-test`
- **Port:** `3307` (to avoid conflicts with local MySQL on 3306)
- **Connection String:** `mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql`
- **Database:** `test_fraiseql`
- **User:** `fraiseql_test`
- **Password:** `fraiseql_test_password`

**Test Views:**

- `v_user` - Same schema as PostgreSQL
- `v_post` - Same schema as PostgreSQL
- `v_product` - Same schema as PostgreSQL

**Note:** MySQL uses `JSON` type (not `JSONB`). SQL generation differs slightly.

### SQLite Test Database

SQLite tests use local file-based databases:

```rust
#[tokio::test]
async fn test_sqlite_query() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    // ...
}
```

No Docker container needed (file-based).

---

## Test Categories

### Unit Tests (Fast, No Database Required)

```bash
# Run all unit tests (excluding integration tests)
cargo test --lib

# Run unit tests for specific module
cargo test -p fraiseql-core db::where_clause
```

**What's tested:**

- WHERE clause AST construction
- WHERE operator validation
- SQL generation (without execution)
- Type conversions
- Pool metrics calculation

### Integration Tests (Require Docker)

```bash
# Run all integration tests
cargo test -- --ignored

# Run integration tests with output
cargo test -- --ignored --nocapture
```

**What's tested:**

- Database connections
- Query execution
- JSONB projection
- All 50+ WHERE operators
- Health checks
- Connection pooling

### End-to-End Tests

```bash
# Run E2E tests (HTTP server + database)
cargo test --test e2e -- --ignored

# Run E2E tests with output
cargo test --test e2e -- --ignored --nocapture
```

**What's tested:**

- Complete GraphQL query execution flows
- HTTP request/response handling
- Authentication and authorization
- Error handling and edge cases
- Multi-database scenarios

---

## Environment Variables

Override default connection strings:

```bash
# PostgreSQL
export FRAISEQL_TEST_POSTGRES_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"

# PostgreSQL + pgvector
export FRAISEQL_TEST_POSTGRES_VECTOR_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5434/test_fraiseql_vector"

# MySQL
export FRAISEQL_TEST_MYSQL_URL="mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql"

# SQLite
export FRAISEQL_TEST_SQLITE_URL=":memory:"
```

---

## Troubleshooting

### Databases not starting

```bash
# Check logs
docker compose -f docker-compose.test.yml logs postgres-test

# Check health status
docker compose -f docker-compose.test.yml ps

# Restart databases
docker compose -f docker-compose.test.yml restart
```

### Port conflicts

If ports 5433, 5434, or 3307 are already in use:

```bash
# Check what's using the port
lsof -i :5433

# Edit docker-compose.test.yml and change ports
```

### Permission errors

```bash
# Remove volumes and recreate
docker compose -f docker-compose.test.yml down -v
docker compose -f docker-compose.test.yml up -d
```

### Tests timing out

Increase test timeout:

```rust
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_with_longer_timeout() {
    // Test code
}
```

Or run with longer timeout:

```bash
RUST_TEST_TIMEOUT=300 cargo test -- --ignored
```

### Connection refused errors

Make sure databases are fully initialized:

```bash
# Wait for health checks to pass
docker compose -f docker-compose.test.yml ps

# Should show "(healthy)" status:
# fraiseql-postgres-test  ... Up (healthy)
```

---

## Test Data

### Reset Test Data

```bash
# Stop containers and remove volumes
docker compose -f docker-compose.test.yml down -v

# Start fresh
docker compose -f docker-compose.test.yml up -d

# Verify data is reset
docker compose -f docker-compose.test.yml exec postgres-test \
  psql -U fraiseql_test -d test_fraiseql -c "SELECT COUNT(*) FROM v_user;"
```

### Add Custom Test Data

Edit the initialization scripts:

- PostgreSQL: `tests/sql/postgres/init.sql`
- PostgreSQL + pgvector: `tests/sql/postgres/init-vector.sql`
- MySQL: `tests/sql/mysql/init.sql`

Then recreate containers:

```bash
docker compose -f docker-compose.test.yml down
docker compose -f docker-compose.test.yml up -d
```

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_USER: fraiseql_test
          POSTGRES_PASSWORD: fraiseql_test_password
          POSTGRES_DB: test_fraiseql
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5433:5432

    steps:
      - uses: actions/checkout@v3
      - uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Run unit tests
        run: cargo test --lib

      - name: Run integration tests
        run: cargo test -- --ignored
        env:
          FRAISEQL_TEST_POSTGRES_URL: postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql
```

### Local Development Workflow

```bash
# 1. Start databases (once per dev session)
docker compose -f docker-compose.test.yml up -d

# 2. Watch for changes and run tests
cargo watch -x 'test --lib'

# 3. Run integration tests manually when needed
cargo test -- --ignored

# 4. Stop databases when done
docker compose -f docker-compose.test.yml down
```

---

## Performance Benchmarks

Use Criterion for performance testing:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench where_generation

# View results
# Open target/criterion/report/index.html
```

**What's benchmarked:**

- WHERE clause generation performance
- Query execution latency
- Schema compilation time
- Connection pool operations

---

## Test Coverage

```bash
# Generate coverage report
make coverage

# Or manually with llvm-cov
cargo llvm-cov --html --output-dir coverage

# View report
xdg-open coverage/index.html
```

**Target:** 85%+ line coverage across all modules

---

## Database Cleanup

```bash
# Remove all test containers and volumes
docker compose -f docker-compose.test.yml down -v

# Remove dangling volumes
docker volume prune

# Remove test networks
docker network prune
```

---

## Summary

| Test Type | Command | Database Required |
|-----------|---------|-------------------|
| Unit tests | `cargo test --lib` | ❌ No |
| Integration tests | `cargo test -- --ignored` | ✅ Yes (Docker) |
| End-to-end tests | `cargo test --test e2e -- --ignored` | ✅ Yes (Docker) |
| Benchmarks | `cargo bench` | ❌ No |
| Coverage | `make coverage` or `cargo llvm-cov --html` | ❌ No |

**Quick Reference:**

```bash
# Full test cycle
docker compose -f docker-compose.test.yml up -d
cargo test --lib              # Unit tests
cargo test -- --ignored       # Integration tests
docker compose -f docker-compose.test.yml down
```
