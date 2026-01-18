# Phase 7.3-7.6: Stabilization - Real-World Testing, Error Refinement, CI/CD, and Documentation

## Overview

After completing performance profiling (7.1) and security audit (7.2), this phase focuses on:

1. **Phase 7.3**: Real-World Testing (staging database, load/stress testing)
2. **Phase 7.4**: Error Message Refinement (actionable errors, user guidance)
3. **Phase 7.5**: CI/CD Improvements (GitHub Actions enhancements, Docker, release automation)
4. **Phase 7.6**: Documentation Polish (API docs, examples, troubleshooting guide)

**Goal**: Transform fraiseql-wire from a solid MVP into a production-ready, well-documented library with battle-tested error handling and streamlined release process.

---

## Phase 7.3: Real-World Testing

### Objective

Validate fraiseql-wire against realistic data volumes, edge cases, and failure scenarios to discover and fix issues before production deployment.

### Prerequisites

- ✅ Performance benchmarks established (Phase 7.1)
- ✅ Security audit passed (Phase 7.2)
- Postgres 17 running with test database

### Tasks

#### 7.3.1 Staging Database Setup

**Objective**: Create a realistic test database with various JSON shapes and data volumes.

**Files to Create/Modify**:

- `tests/fixtures/schema.sql` — Create staging schema with v_* views
- `tests/fixtures/seed_data.sql` — Realistic JSON data (small, medium, large, deeply nested)
- `.github/workflows/staging-tests.yml` — CI workflow for staging tests

**Implementation Steps**:

1. Create `tests/fixtures/schema.sql`:

   ```sql
   -- Staging schema for realistic testing
   CREATE SCHEMA IF NOT EXISTS test_staging;

   -- Entity table with JSON data plane
   CREATE TABLE test_staging.projects (
       id UUID PRIMARY KEY,
       created_at TIMESTAMP NOT NULL,
       data JSONB NOT NULL
   );

   -- View for fraiseql-wire testing
   CREATE VIEW test_staging.v_projects AS
   SELECT id, data FROM test_staging.projects;

   -- Nested/complex data shape
   CREATE TABLE test_staging.complex_entities (
       id UUID PRIMARY KEY,
       data JSONB NOT NULL
   );

   CREATE VIEW test_staging.v_complex_entities AS
   SELECT id, data FROM test_staging.complex_entities;
   ```

2. Create `tests/fixtures/seed_data.sql`:
   - Small JSON (< 1KB)
   - Medium JSON (1-100KB)
   - Large JSON (100KB-1MB)
   - Deeply nested JSON (10+ levels)
   - Various field types (strings, numbers, booleans, arrays, objects)

3. Create test data generator script:

   ```bash
   tests/generate_test_data.sh
   ```

   - Generate realistic data volumes
   - Insert 1K, 100K, 1M row variations

**Acceptance Criteria**:

- [ ] Schema with 3-4 different entity shapes
- [ ] Seed data includes small, medium, large, and nested JSON
- [ ] Database can be populated in < 30 seconds
- [ ] Data generator script is idempotent
- [ ] Total test database size < 5GB

**Verification**:

```bash
# Manual: Set up schema and seed
psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql
psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql

# Verify data loaded
psql -U postgres -d fraiseql_test -c "SELECT COUNT(*) FROM test_staging.v_projects;"
```

---

#### 7.3.2 Load Testing

**Objective**: Test throughput and memory stability under sustained load (multiple concurrent connections, high row volumes).

**Files to Create**:

- `tests/load_tests.rs` — Load testing with various concurrency levels
- `benches/load_benchmark.rs` — Optional: integration benchmark suite

**Implementation Steps**:

1. Create `tests/load_tests.rs`:

   ```rust
   #[tokio::test]
   #[ignore]
   async fn test_load_high_concurrency() {
       // Create 10+ concurrent connections
       // Each streams 100K+ rows
       // Monitor memory and throughput
       // Assert sustained performance
   }

   #[tokio::test]
   #[ignore]
   async fn test_large_result_set_memory() {
       // Stream 1M+ rows with different chunk sizes
       // Monitor peak memory
       // Verify O(chunk_size) memory bound
   }

   #[tokio::test]
   #[ignore]
   async fn test_sustained_streaming() {
       // Run for 5+ minutes continuously
       // Monitor for memory leaks
       // Verify stable performance
   }
   ```

2. Load test scenarios:
   - 5 concurrent connections × 100K rows each
   - 10 concurrent connections × 50K rows each
   - 1 connection × 1M rows (memory stress)
   - 3-hour sustained streaming test

**Metrics to Collect**:

- Peak memory usage
- Throughput (rows/sec)
- CPU utilization
- Connection overhead
- GC pauses (if applicable)

**Acceptance Criteria**:

- [ ] 10 concurrent connections maintained without errors
- [ ] Memory stays within O(chunk_size) + 100MB overhead
- [ ] Throughput consistent across concurrency levels
- [ ] No memory leaks over 1-hour test
- [ ] Performance degrades gracefully under extreme load

**Verification**:

```bash
# Run load tests (manual)
cargo test --test load_tests -- --ignored --nocapture --test-threads=1

# Collect metrics with /usr/bin/time
/usr/bin/time -v cargo test --test load_tests -- --ignored
```

---

#### 7.3.3 Stress Testing

**Objective**: Test fault tolerance under adverse conditions (connection drops, network delays, database unavailability, query timeouts).

**Files to Create**:

- `tests/stress_tests.rs` — Failure scenario testing

**Implementation Steps**:

1. Test failure scenarios:

   ```rust
   #[tokio::test]
   #[ignore]
   async fn test_connection_drop_during_query() {
       // Start query
       // Kill Postgres connection mid-stream
       // Verify error is caught and propagated
       // Verify client can reconnect
   }

   #[tokio::test]
   #[ignore]
   async fn test_network_timeout() {
       // Use network simulation (tc/toxiproxy)
       // Introduce 10+ second latency
       // Verify timeout behavior
   }

   #[tokio::test]
   #[ignore]
   async fn test_database_restart() {
       // Start query against healthy Postgres
       // Restart Postgres mid-query
       // Verify clean error handling
       // Verify reconnect works
   }

   #[tokio::test]
   #[ignore]
   async fn test_partial_json_parsing() {
       // Insert malformed JSON
       // Verify decode error is caught
       // Verify stream terminates cleanly
   }
   ```

2. Stress conditions:
   - Sudden connection close (socket close without warning)
   - Network partition (no connectivity for 30+ seconds)
   - Database restart (SIGTERM Postgres)
   - Malformed data (invalid JSON)
   - Resource exhaustion (very large JSON rows)

**Acceptance Criteria**:

- [ ] All stress scenarios produce actionable error messages
- [ ] No panics or crashes
- [ ] Stream cancellation works in all scenarios
- [ ] Client can recover and reconnect
- [ ] Resource cleanup is guaranteed (no fd leaks)

**Verification**:

```bash
# Run stress tests
cargo test --test stress_tests -- --ignored --nocapture

# Manual network simulation (if using toxiproxy)
toxiproxy-cli toxic add -t latency -a 10000 postgres_master
```

---

### Phase 7.3 Deliverables

**Documentation**:

- `TESTING_GUIDE.md` — How to run load/stress tests manually
- Comments in test files explaining each scenario

**Code**:

- `tests/load_tests.rs` — Full load testing suite
- `tests/stress_tests.rs` — Failure scenario testing
- `tests/fixtures/schema.sql` — Staging database schema
- `tests/fixtures/seed_data.sql` — Seed data generator
- `.github/workflows/staging-tests.yml` — CI workflow (optional, can be manual)

**Metrics**:

- Baseline memory/throughput under load
- Fault tolerance coverage matrix

---

## Phase 7.4: Error Message Refinement

### Objective

Review all error messages for clarity, actionability, and user friendliness. Add guides for common error scenarios.

### Prerequisites

- ✅ Phases 7.1, 7.2, 7.3 complete
- Understanding of common user pain points

### Tasks

#### 7.4.1 Error Message Audit

**Objective**: Review every error message for clarity and actionability.

**Files to Review**:

- `src/error.rs` — All error types and messages
- `src/client/fraise_client.rs` — Client-level errors
- `src/connection/conn.rs` — Connection errors
- `src/protocol/decode.rs` — Protocol errors
- `src/json/validate.rs` — JSON validation errors

**Process**:

1. For each error variant, evaluate:
   - Is the message clear to a user who doesn't know internals?
   - Does it explain what went wrong?
   - Does it suggest how to fix it?
   - Is the phrasing user-friendly (not jargon)?

2. Common error scenarios to enhance:

   ```rust
   // Before (unclear):
   Error::Connection("connection failed")

   // After (actionable):
   Error::Connection("failed to connect to postgres://localhost:5432: connection refused. Is Postgres running?")

   Error::Config("invalid connection string")
   Error::Config("invalid connection string 'postgres://user@/db': missing hostname for TCP connections")

   Error::InvalidSchema("invalid result schema")
   Error::InvalidSchema("query returned 2 columns instead of 1. fraiseql-wire supports only single-column SELECT data queries")
   ```

**Changes to Make in `src/error.rs`**:

1. Add context fields to error variants:

   ```rust
   #[derive(Debug, Error)]
   pub enum Error {
       #[error("connection error: {message}")]
       Connection {
           message: String,
           #[source]
           source: Option<Box<dyn std::error::Error>>,
       },
       // ... etc
   }
   ```

2. Add helper methods with context:

   ```rust
   impl Error {
       pub fn connection_refused(addr: &str) -> Self {
           Error::Connection {
               message: format!(
                   "failed to connect to {}: connection refused. Is Postgres running?",
                   addr
               ),
               source: None,
           }
       }
   }
   ```

**Acceptance Criteria**:

- [ ] Every error variant is reviewed and audited
- [ ] Error messages are clear and actionable
- [ ] Error variants include helpful context
- [ ] All existing tests still pass

**Verification**:

```bash
cargo test --lib error_tests
```

---

#### 7.4.2 Common Error Scenarios Documentation

**Objective**: Create a troubleshooting guide covering the most common issues users will face.

**File to Create**:

- `TROUBLESHOOTING.md` — Common errors and solutions

**Structure**:

```markdown
# Troubleshooting Guide for fraiseql-wire

## Connection Errors

### Error: "connection refused"
- **Cause**: Postgres is not running or not listening on the specified address
- **Solution**:
  1. Verify Postgres is running: `pg_isready`
  2. Check hostname/port: `psql -h localhost -p 5432 -U postgres`
  3. Verify firewall allows connections

### Error: "authentication failed"
- **Cause**: Wrong username/password
- **Solution**:
  1. Verify credentials with `psql -U myuser -W`
  2. Check connection string format
  3. Verify user has login privilege

### Error: "connection closed"
- **Cause**: Postgres closed the connection unexpectedly
- **Solution**:
  1. Check Postgres logs for errors
  2. Verify connection timeout is appropriate
  3. Check for network issues

## Query Errors

### Error: "invalid result schema"
- **Cause**: Query doesn't return exactly one column named `data`
- **Solution**:
  1. Use `SELECT data FROM ...` not `SELECT *`
  2. Ensure data column is JSON/JSONB type
  3. Check view definition matches v_entity pattern

### Error: "sql error"
- **Cause**: Query syntax error or table not found
- **Solution**:
  1. Test query directly with `psql`
  2. Verify table/view exists with `\dv`
  3. Check WHERE clause syntax

## Performance Issues

### Throughput is lower than expected
- **Cause**: Network, chunk size, or predicate issues
- **Solution**:
  1. Increase chunk_size (see PERFORMANCE_TUNING.md)
  2. Push WHERE clause to SQL (not Rust predicates)
  3. Verify network latency to Postgres

### Memory usage is high
- **Cause**: chunk_size is too large or query returns large JSON values
- **Solution**:
  1. Reduce chunk_size
  2. Add WHERE clause to filter data
  3. Check for very large JSON objects in data

## Network Issues

### Queries time out
- **Cause**: Slow network or large result set
- **Solution**:
  1. Check network latency: `ping postgres-host`
  2. Increase timeout in connection config
  3. Add WHERE clause to reduce data volume

## Developer Issues

### "invalid connection state"
- **Cause**: Trying to use connection after it's closed
- **Solution**:
  1. Create new FraiseClient for each query
  2. Don't reuse connection after error
  3. Drop stream before starting new query
```

**Acceptance Criteria**:

- [ ] 10+ common error scenarios documented
- [ ] Each includes cause, symptoms, and solutions
- [ ] Cross-references to PERFORMANCE_TUNING.md and SECURITY.md
- [ ] Examples are copy-paste ready

---

#### 7.4.3 Error Tests

**Objective**: Add unit tests for error creation and categorization.

**Modifications to `src/error.rs` tests section**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_refused_message() {
        let err = Error::connection_refused("localhost:5432");
        assert!(err.to_string().contains("connection refused"));
        assert!(err.to_string().contains("Is Postgres running?"));
    }

    #[test]
    fn test_invalid_schema_message() {
        let err = Error::invalid_schema_column_count(2);
        assert!(err.to_string().contains("2 columns"));
        assert!(err.to_string().contains("single-column"));
    }

    #[test]
    fn test_error_categories() {
        // All error variants should have a category
        // Categories should be stable for metrics
    }

    #[test]
    fn test_is_retriable_consistency() {
        // Transient errors should be retriable
        // Permanent errors should not be
    }
}
```

**Acceptance Criteria**:

- [ ] All error helper methods tested
- [ ] Error message clarity verified
- [ ] Categorization logic validated
- [ ] Retriability logic correct

---

### Phase 7.4 Deliverables

**Documentation**:

- `TROUBLESHOOTING.md` — Common error scenarios and solutions (~300-400 lines)
- Updated comments in `src/error.rs`

**Code**:

- Enhanced error messages in `src/error.rs`
- Additional error helper methods
- Comprehensive error tests

---

## Phase 7.5: CI/CD Improvements

### Objective

Enhance GitHub Actions workflows, Docker support, and release automation for smooth releases.

### Prerequisites

- ✅ Phases 7.1-7.4 complete
- GitHub Actions workflow experience
- Docker knowledge

### Tasks

#### 7.5.1 GitHub Actions Enhancements

**Current Status**:

- ✅ Basic CI (build, test, clippy, fmt)
- ✅ Integration tests with Postgres
- ⚠️ Benchmarks are nightly-only
- ❌ Performance tracking
- ❌ Coverage reporting
- ❌ Automated releases

**File to Modify**:

- `.github/workflows/ci.yml` — Enhance main CI

**Enhancements**:

1. **Add code coverage reporting**:

   ```yaml
   - name: Install tarpaulin
     run: cargo install cargo-tarpaulin

   - name: Generate coverage
     run: cargo tarpaulin --out Xml

   - name: Upload coverage to Codecov
     uses: codecov/codecov-action@v3
     with:
       files: ./cobertura.xml
   ```

2. **Add security audit**:

   ```yaml
   - name: Security audit
     run: cargo audit --deny warnings
   ```

3. **Add MSRV (Minimum Supported Rust Version) test**:

   ```yaml
   - name: Test MSRV
     uses: dtolnay/rust-toolchain@1.70
     run: cargo test --lib
   ```

4. **Improve integration test setup**:
   - Use `sqlx prepare` for query validation
   - Initialize schema before tests
   - Better error messages on failure

5. **Add performance regression detection** (optional):

   ```yaml
   - name: Run micro-benchmarks
     run: cargo bench --bench micro_benchmarks -- --output-format bencher | tee output.txt

   - name: Store benchmark result
     uses: benchmark-action/github-action-benchmark@v1
     with:
       tool: 'cargo'
       output-file-path: output.txt
   ```

**New Workflow File**: `.github/workflows/release.yml`

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  release:
    name: Release
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build
        run: cargo build --release

      - name: Test
        run: cargo test --release

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          body: |
            ## Changes
            See CHANGELOG.md for full details
          draft: false

      - name: Publish to crates.io
        run: cargo publish --token ${{ secrets.CARGO_TOKEN }}
```

**Acceptance Criteria**:

- [ ] Coverage reporting added and working
- [ ] Security audit in CI with no warnings
- [ ] MSRV test passing
- [ ] Release workflow created and tested
- [ ] All workflows documented in CONTRIBUTING.md

**Verification**:

```bash
# Test workflows locally with act
act -l  # List all workflows
act push -b main  # Simulate push event
```

---

#### 7.5.2 Docker Improvements

**Current Status**:

- Dockerfile exists
- Single-platform build only

**Files to Modify**:

- `Dockerfile` — Multi-platform support
- `docker-compose.yml` — Development environment (create if missing)

**Enhancements**:

1. **Multi-platform Docker image**:

   ```dockerfile
   # Dockerfile
   FROM rust:latest as builder
   WORKDIR /app
   COPY . .
   RUN cargo build --release --target x86_64-unknown-linux-gnu
   RUN cargo build --release --target aarch64-unknown-linux-gnu

   FROM debian:bookworm-slim
   RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
   COPY --from=builder /app/target/*/release/fraiseql-wire /usr/local/bin/
   ENTRYPOINT ["/usr/local/bin/fraiseql-wire"]
   ```

2. **Development docker-compose.yml**:

   ```yaml
   version: '3.8'
   services:
     postgres:
       image: postgres:17-alpine
       environment:
         POSTGRES_PASSWORD: postgres
       ports:
         - "5432:5432"

     test:
       build: .
       command: cargo test --test integration -- --ignored
       depends_on:
         - postgres
       environment:
         POSTGRES_HOST: postgres
   ```

3. **GitHub Actions for Docker images**:

   ```yaml
   - name: Set up QEMU
     uses: docker/setup-qemu-action@v2

   - name: Set up Docker Buildx
     uses: docker/setup-buildx-action@v2

   - name: Build and push
     uses: docker/build-push-action@v4
     with:
       platforms: linux/amd64,linux/arm64
       tags: lionel/fraiseql-wire:latest
   ```

**Acceptance Criteria**:

- [ ] Multi-platform build tested (amd64, arm64)
- [ ] docker-compose.yml works for development
- [ ] GitHub Actions Docker workflow builds successfully
- [ ] Published image is documented in README

---

#### 7.5.3 Release Automation

**Objective**: Streamline the release process to crates.io and GitHub Releases.

**Files to Create/Modify**:

- `.github/workflows/release.yml` — Release workflow
- `scripts/publish.sh` — Local release script
- `CONTRIBUTING.md` — Release procedure documentation

**Release Procedure**:

1. **Version bump** (semantic versioning):

   ```bash
   # Edit Cargo.toml
   # version = "0.2.0"

   # Update CHANGELOG.md with changes
   # Commit changes
   git add Cargo.toml CHANGELOG.md
   git commit -m "chore: bump version to 0.2.0"
   ```

2. **Automated release**:

   ```bash
   # Push tag
   git tag -a v0.2.0 -m "Release 0.2.0"
   git push origin v0.2.0

   # GitHub Actions handles:
   # 1. Build and test
   # 2. Create GitHub Release (from CHANGELOG)
   # 3. Publish to crates.io
   ```

3. **Verification**:
   - Check GitHub Releases page
   - Verify crates.io has new version
   - Spot-check documentation

**Release Script** (`scripts/publish.sh`):

```bash
#!/bin/bash
set -e

VERSION=$1
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

# Verify on main branch
git checkout main
git pull origin main

# Verify no uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo "Error: uncommitted changes"
    exit 1
fi

# Update version
sed -i "s/^version = .*/version = \"$VERSION\"/" Cargo.toml

# Build and test
cargo build --release
cargo test --lib
cargo clippy -- -D warnings

# Create tag
git add Cargo.toml
git commit -m "chore: bump version to $VERSION"
git tag -a "v$VERSION" -m "Release $VERSION"

# Push
git push origin main
git push origin "v$VERSION"

echo "Release $VERSION pushed. GitHub Actions will handle publishing."
```

**Acceptance Criteria**:

- [ ] Release workflow tested with dry-run
- [ ] Release script is idempotent
- [ ] GitHub Release created with proper format
- [ ] crates.io receives new version
- [ ] Release procedure documented

---

### Phase 7.5 Deliverables

**CI/CD**:

- Enhanced `.github/workflows/ci.yml` with coverage, audit, MSRV
- New `.github/workflows/release.yml` for automated releases
- Updated `Dockerfile` with multi-platform support
- `docker-compose.yml` for development

**Scripts**:

- `scripts/publish.sh` — Release automation script

**Documentation**:

- Updated `CONTRIBUTING.md` with CI/CD and release procedures

---

## Phase 7.6: Documentation Polish

### Objective

Ensure comprehensive, clear, accessible documentation for users and contributors.

### Prerequisites

- ✅ Phases 7.1-7.5 complete
- All code and tests finalized

### Tasks

#### 7.6.1 API Documentation Review

**Objective**: Ensure all public items are well-documented with examples.

**Files to Review/Enhance**:

- `src/lib.rs` — Crate-level documentation
- `src/client/fraise_client.rs` — Client API docs
- `src/client/query_builder.rs` — Query builder docs
- All public types and methods

**Requirements**:

1. Every public item must have doc comments:

   ```rust
   /// Executes a query and returns a stream of JSON values.
   ///
   /// # Examples
   ///
   /// ```rust,no_run
   /// # async fn example() -> Result<()> {
   /// let client = FraiseClient::connect("postgres://localhost/db").await?;
   /// let mut stream = client
   ///     .query("projects")
   ///     .where_sql("status = 'active'")
   ///     .execute()
   ///     .await?;
   ///
   /// while let Some(result) = stream.next().await {
   ///     let value = result?;
   ///     println!("{}", value);
   /// }
   /// # Ok(())
   /// # }
   /// ```
   ///
   /// # Errors
   ///
   /// Returns `Error::Connection` if the database connection fails.
   /// Returns `Error::Sql` if the query is invalid.
   pub async fn execute(self) -> Result<...> { ... }
   ```

2. Documentation should include:
   - Brief description
   - Practical examples (copy-paste ready)
   - Error conditions
   - Performance considerations
   - Panics (if any)

3. Run documentation check:

   ```bash
   cargo doc --no-deps --open
   RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
   ```

**Acceptance Criteria**:

- [ ] Every public function documented
- [ ] Every public type documented
- [ ] All examples compile and work
- [ ] No doc warnings in build
- [ ] Examples demonstrate common patterns

**Verification**:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo test --doc
```

---

#### 7.6.2 Example Programs

**Objective**: Create practical, runnable examples for common use cases.

**Files to Create**:

- `examples/basic_query.rs` — Simple single query
- `examples/filtering.rs` — WHERE clause + predicates
- `examples/ordering.rs` — ORDER BY usage
- `examples/streaming.rs` — Large result handling
- `examples/error_handling.rs` — Error scenarios
- `examples/connection_pooling.rs` — Connection management (if applicable)

**Example Structure** (`examples/basic_query.rs`):

```rust
//! Basic query example
//!
//! Run with:
//! ```
//! POSTGRES_HOST=localhost POSTGRES_USER=postgres \
//! POSTGRES_PASSWORD=postgres POSTGRES_DB=fraiseql_test \
//! cargo run --example basic_query
//! ```

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse connection string from environment
    let conn_string = format!(
        "postgres://{}:{}@{}/{}",
        env!("POSTGRES_USER", "postgres"),
        env!("POSTGRES_PASSWORD", "postgres"),
        env!("POSTGRES_HOST", "localhost"),
        env!("POSTGRES_DB", "fraiseql_test"),
    );

    // Connect to Postgres
    let client = FraiseClient::connect(&conn_string).await?;

    // Execute query and stream results
    let mut stream = client
        .query("projects")
        .execute()
        .await?;

    // Process results
    while let Some(result) = stream.next().await {
        let value = result?;
        println!("{}", value);
    }

    Ok(())
}
```

**Examples to Create**:

1. **basic_query.rs** — Query without predicates
2. **filtering.rs** — WHERE + Rust predicates
3. **ordering.rs** — ORDER BY + pagination patterns
4. **streaming.rs** — Handling large streams
5. **error_handling.rs** — Error recovery patterns
6. **performance.rs** — Tuning chunk_size, predicates

**Acceptance Criteria**:

- [ ] 5+ examples covering common use cases
- [ ] All examples compile and run
- [ ] Examples have clear comments
- [ ] Examples demonstrate best practices
- [ ] Running instructions are clear

**Verification**:

```bash
for example in examples/*.rs; do
    cargo run --example $(basename $example .rs)
done
```

---

#### 7.6.3 Comprehensive README Update

**Objective**: Ensure README is current, clear, and guides users through basic usage.

**File to Modify**: `README.md`

**Sections to Review/Add**:

1. **Quick Start** (copy-paste ready)

   ```markdown
   ## Quick Start

   Add to Cargo.toml:
   ```toml
   fraiseql-wire = "0.1"
   ```

   Basic example:

   ```rust
   use fraiseql_wire::FraiseClient;
   use futures::stream::StreamExt;

   #[tokio::main]
   async fn main() -> Result<()> {
       let client = FraiseClient::connect("postgres://localhost/mydb").await?;
       let mut stream = client.query("users").execute().await?;

       while let Some(result) = stream.next().await {
           println!("{}", result?);
       }
       Ok(())
   }
   ```

2. **Features** — Updated with Phase 7 additions
   - Performance (with benchmark numbers)
   - Memory bounds (O(chunk_size))
   - TLS roadmap (Phase 8)
   - Comparison with tokio-postgres

3. **Installation & Setup**
   - System requirements (Rust 1.70+, Postgres 10+)
   - Development setup (Docker)
   - Running tests

4. **Guide Navigation**

   ```markdown
   ## Learning Resources

   - **[Getting Started](DEVELOPMENT.md)** — Development setup
   - **[Performance Tuning](PERFORMANCE_TUNING.md)** — Optimization guide
   - **[Security Guide](SECURITY.md)** — Security considerations
   - **[Troubleshooting](TROUBLESHOOTING.md)** — Common issues
   - **[Examples](examples/)** — Runnable code samples
   - **[Contributing](CONTRIBUTING.md)** — How to contribute
   ```

5. **Feature Table** (from COMPARISON_GUIDE.md)

   ```markdown
   | Feature | fraiseql-wire | tokio-postgres | Notes |
   |---------|---------------|----------------|-------|
   | Streaming | ✅ | ✅ | Single column only |
   | Memory | O(chunk) | O(result) | 1000x-20000x savings |
   | Setup time | 5-10ms | 10-20ms | Minimal overhead |
   | TLS | ⏳ (Phase 8) | ✅ | WIP |
   | Connection pooling | ⏳ (Phase 8) | ✅ | Future feature |
   ```

**Acceptance Criteria**:

- [ ] README clearly explains what fraiseql-wire is for
- [ ] Quick start example is copy-paste ready
- [ ] Performance characteristics clearly stated
- [ ] Links to other docs are correct
- [ ] Benchmarks and comparisons up to date

---

#### 7.6.4 Contributing Guide Update

**File to Modify**: `CONTRIBUTING.md`

**Sections to Add/Update**:

1. **Architecture Overview**
   - Module structure
   - Data flow diagram
   - Design principles

2. **Development Workflow**
   - Setting up environment
   - Running tests
   - Code style
   - Submitting PRs

3. **Testing Strategy**
   - Unit tests
   - Integration tests
   - Load testing
   - Stress testing

4. **Release Procedure**
   - Version bumping
   - GitHub release creation
   - crates.io publishing
   - Changelog format

5. **Adding Features**
   - Decision framework (from ROADMAP)
   - Scope validation
   - Hard invariants
   - Review process

**Example Section**:

```markdown
## Adding Features

Before implementing a feature, check:

1. **Is it in scope?** (see ROADMAP.md Decision Framework)
   - Single column JSON output
   - Streaming-first design
   - No arbitrary SQL

2. **Update architecture?** Document in CLAUDE.md

3. **Add tests first** (TDD workflow)
   - Unit tests for logic
   - Integration tests with Postgres
   - Load tests if performance-critical

4. **Document** with examples and doc comments

5. **Submit PR** with clear description
```

**Acceptance Criteria**:

- [ ] Contributing guide is comprehensive
- [ ] Workflow is clear for new contributors
- [ ] Release procedure documented
- [ ] Decision framework is accessible

---

#### 7.6.5 Documentation Audit

**Objective**: Ensure all documentation is correct, complete, and consistent.

**Files to Audit**:

- `ROADMAP.md` — Update Phase 7 status ✅
- `DEVELOPMENT.md` — Setup instructions
- `PERFORMANCE_TUNING.md` — Optimization guidance
- `SECURITY.md` — Security considerations
- `SECURITY_AUDIT.md` — Technical audit
- `BENCHMARKING.md` — Benchmark instructions
- `benches/COMPARISON_GUIDE.md` — Comparison with tokio-postgres

**Checklist**:

- [ ] All links are valid
- [ ] Code examples are current
- [ ] Version numbers are consistent
- [ ] Phase status is accurate
- [ ] No broken markdown
- [ ] Spell-check passes
- [ ] Tone is consistent

**Verification**:

```bash
# Check links
cargo install markdown-link-check
markdown-link-check README.md ROADMAP.md *.md

# Check markdown syntax
npx markdownlint *.md

# Spell check
cargo install crate-ci-spell-check
cargo +nightly spellcheck --code 1
```

**Acceptance Criteria**:

- [ ] All documentation reviewed
- [ ] Links verified working
- [ ] Consistency checks pass
- [ ] No formatting issues

---

### Phase 7.6 Deliverables

**Documentation**:

- Updated `README.md` with quick start and features
- Enhanced `CONTRIBUTING.md` with workflow
- API documentation (doc comments)
- `examples/` directory with 5+ runnable examples
- All markdown files audited and validated

**Code**:

- Complete doc comments on all public items
- Doc tests (compile and run examples)

**Verification Scripts**:

- Documentation build passes without warnings
- All examples compile and run
- All links valid

---

## Success Criteria for Phases 7.3-7.6

### Overall Stability

- ✅ Comprehensive test coverage (unit, integration, load, stress)
- ✅ All error messages are actionable
- ✅ CI/CD pipelines are robust and automated
- ✅ Documentation is complete and accessible

### Quantitative Metrics

| Metric | Target | Verification |
|--------|--------|--------------|
| Error message clarity | 100% | Manual review + tests |
| Test coverage | > 85% | `cargo tarpaulin` |
| Documentation completeness | 100% public items | `RUSTDOCFLAGS="-D warnings"` |
| CI passing | 100% | GitHub Actions status |
| Integration tests passing | 100% | Postgres-based tests |
| Load test throughput stability | ±5% variance | Repeated runs |
| Memory under load | O(chunk_size) + 100MB | Profiling |

### Qualitative Metrics

- Error scenarios are well-handled with clear guidance
- New users can get started in < 10 minutes
- Contributors can understand and extend the codebase
- Documentation covers common use cases and troubleshooting
- Release process is streamlined and repeatable

---

## Timeline & Effort Estimate

| Phase | Components | Effort |
|-------|------------|--------|
| **7.3** | Staging, load, stress tests | 3-4 days |
| **7.4** | Error audit, messages, guide | 2-3 days |
| **7.5** | CI/CD, Docker, release | 2-3 days |
| **7.6** | Docs, examples, polish | 2-3 days |
| **Total** | 7.3-7.6 Complete | 9-13 days |

---

## Next Phase: Phase 8 (Feature Expansion)

After Phase 7 completes:

1. **Gather user feedback** (production trial)
2. **Prioritize features**:
   - TLS support (highest priority for cloud deployments)
   - Connection pooling (needed for applications)
   - SCRAM authentication (security improvement)
   - Typed streaming (developer experience)

3. **Create Phase 8 plan** (separate document)

---

## Appendix: Rollout Sequence

### Week 1

- 7.3.1: Staging database setup
- 7.3.2: Load testing framework

### Week 2

- 7.3.3: Stress testing
- 7.4.1: Error audit

### Week 3

- 7.4.2: Troubleshooting guide
- 7.5.1: CI/CD enhancements

### Week 4

- 7.5.2: Docker improvements
- 7.5.3: Release automation
- 7.6.1: API documentation

### Week 5

- 7.6.2: Examples
- 7.6.3: README update
- 7.6.4: CONTRIBUTING guide
- 7.6.5: Final audit

### Outcome

**fraiseql-wire v0.1.x** is:

- ✅ Battle-tested (phases 7.3-7.4)
- ✅ Well-documented (phase 7.6)
- ✅ Easy to release and deploy (phase 7.5)
- ✅ Production-ready for adoption

Ready for **Phase 8: Feature Expansion** and real-world integration!
