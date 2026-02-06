# Phase 0.2: Test Architecture & Infrastructure

**Phase**: 0.2 of 0.5 (Part of Phase 0 - Setup)
**Effort**: 1.5 hours
**Status**: Ready to implement
**Prerequisite**: Phase 0.1 (Clippy)

---

## Objective

Establish comprehensive test infrastructure for all 5 implementation phases:

1. Create test module structure (unit/integration/e2e)
2. Set up test utilities and fixtures
3. Configure database test containers
4. Establish test database lifecycle management
5. Create test helpers for common operations

**Success Criteria**:

- ✅ Test directory structure created
- ✅ Test utilities module available
- ✅ TestDatabase helper working
- ✅ First unit test passing
- ✅ First integration test passing
- ✅ Database container starting/stopping correctly

---

## Why This Matters

**Parallel Development**: Each phase can write tests first (TDD) without waiting for infrastructure

**Isolation**: Tests don't interfere with each other (separate databases per test)

**Speed**: Unit tests run fast (no DB), integration tests run with containers

**Consistency**: All tests follow same patterns, easier to maintain

---

## Test Architecture Overview

```
Unit Tests (60%)           Integration Tests (30%)      E2E Tests (10%)
├─ Pool configuration      ├─ Pool + queries          ├─ Full GraphQL
├─ WHERE clause builder    ├─ Connection lifecycle    ├─ Real database
├─ JSON transformation     ├─ Transaction handling    ├─ Performance
├─ Parameter conversion    ├─ Streaming behavior      └─ Load testing
└─ Type conversions        └─ Error recovery

No external deps           Needs PostgreSQL           Needs full app
~100ms per test           ~1s per test               ~5-10s per test
```

---

## Implementation Steps

### Step 1: Create Test Directory Structure

```bash
# Run these commands to create structure
mkdir -p fraiseql_rs/tests/{unit,integration,e2e,common}
mkdir -p fraiseql_rs/benches
touch fraiseql_rs/tests/common/mod.rs
touch fraiseql_rs/tests/unit/mod.rs
touch fraiseql_rs/tests/integration/mod.rs
touch fraiseql_rs/tests/e2e/mod.rs
```

**Resulting structure**:

```
fraiseql_rs/
├── src/
│   ├── lib.rs
│   ├── db/
│   │   ├── mod.rs
│   │   ├── pool.rs
│   │   │   └── #[cfg(test)] mod tests { ... }
│   │   ├── query.rs
│   │   │   └── #[cfg(test)] mod tests { ... }
│   │   └── types.rs
│   │       └── #[cfg(test)] mod tests { ... }
│   └── ...
│
├── tests/
│   ├── common/
│   │   ├── mod.rs              # Shared test utilities
│   │   ├── database.rs         # TestDatabase helper
│   │   ├── fixtures.rs         # Test data fixtures
│   │   └── assertions.rs       # Custom assertions
│   │
│   ├── unit/                   # Fast tests, no DB
│   │   ├── mod.rs
│   │   ├── pool_config_tests.rs
│   │   ├── where_builder_tests.rs
│   │   ├── type_conversion_tests.rs
│   │   └── json_transform_tests.rs
│   │
│   ├── integration/            # Requires PostgreSQL
│   │   ├── mod.rs
│   │   ├── pool_tests.rs
│   │   ├── query_execution_tests.rs
│   │   ├── where_clause_tests.rs
│   │   ├── transaction_tests.rs
│   │   ├── streaming_tests.rs
│   │   ├── error_recovery_tests.rs
│   │   └── parity_tests.rs     # Rust vs psycopg
│   │
│   ├── e2e/                    # Full GraphQL
│   │   ├── mod.rs
│   │   ├── graphql_queries_tests.rs
│   │   ├── graphql_mutations_tests.rs
│   │   └── performance_tests.rs
│   │
│   └── common.rs               # Import common module
│
└── benches/                    # Criterion benchmarks
    ├── connection_pool.rs
    ├── query_execution.rs
    └── streaming.rs
```

---

### Step 2: Create Common Test Utilities

**File**: `fraiseql_rs/tests/common/mod.rs`

```rust
//! Common test utilities and fixtures
//!
//! This module provides:
//! - TestDatabase container management
//! - Test fixtures and sample data
//! - Custom assertions
//! - Connection helpers

pub mod database;
pub mod fixtures;
pub mod assertions;

pub use database::TestDatabase;
pub use fixtures::*;
pub use assertions::*;

// Re-export commonly used test items
pub use tokio::test;
```

---

### Step 3: Create TestDatabase Helper

**File**: `fraiseql_rs/tests/common/database.rs`

```rust
//! TestDatabase helper for managing test PostgreSQL instances
//!
//! Creates a fresh test database for each test, automatically cleaned up.

use std::sync::Arc;
use testcontainers::clients;
use testcontainers::images::postgres::Postgres;
use tokio_postgres::Client;

/// Manages a test PostgreSQL database instance
#[derive(Clone)]
pub struct TestDatabase {
    inner: Arc<TestDatabaseInner>,
}

struct TestDatabaseInner {
    docker: clients::Cli,
    container: testcontainers::Container<'static, Postgres>,
}

impl TestDatabase {
    /// Create a new test database with default settings
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_config(TestDatabaseConfig::default()).await
    }

    /// Create a test database with custom configuration
    pub async fn with_config(
        config: TestDatabaseConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let docker = clients::Cli::default();

        let image = Postgres::default()
            .with_db_name(&config.db_name)
            .with_user(&config.user)
            .with_password(&config.password);

        let container = docker.run(image);
        let port = container.get_host_port_ipv4(5432);

        let connection_string = format!(
            "postgresql://{}:{}@127.0.0.1:{}/{}",
            config.user, config.password, port, config.db_name
        );

        // Wait for database to be ready
        Self::wait_for_db(&connection_string).await?;

        Ok(TestDatabase {
            inner: Arc::new(TestDatabaseInner {
                docker,
                container,
            }),
        })
    }

    /// Get connection string for this test database
    pub fn connection_string(&self) -> String {
        // Implementation matches container port mapping
        format!(
            "postgresql://{}:{}@127.0.0.1:{}/test_db",
            "postgres", "postgres", 5432
        )
    }

    /// Get a PostgreSQL client for queries
    pub async fn client(&self) -> Result<Client, Box<dyn std::error::Error>> {
        let (client, connection) =
            tokio_postgres::connect(&self.connection_string(), tokio_postgres::tls::NoTls)
                .await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        Ok(client)
    }

    /// Execute a query and return results
    pub async fn query(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, Box<dyn std::error::Error>> {
        let client = self.client().await?;
        Ok(client.query(sql, params).await?)
    }

    /// Execute a statement without returning rows
    pub async fn execute(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let client = self.client().await?;
        Ok(client.execute(sql, params).await?)
    }

    /// Run migrations on test database
    pub async fn migrate(&self, migrations: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        for migration in migrations {
            self.execute(migration, &[]).await?;
        }
        Ok(())
    }

    /// Wait for database to be ready
    async fn wait_for_db(connection_string: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut retries = 30;
        loop {
            match tokio_postgres::connect(
                connection_string,
                tokio_postgres::tls::NoTls,
            )
            .await
            {
                Ok((client, connection)) => {
                    tokio::spawn(async move {
                        let _ = connection.await;
                    });
                    let _ = client.simple_query("SELECT 1").await;
                    return Ok(());
                }
                Err(_) if retries > 0 => {
                    retries -= 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                Err(e) => return Err(Box::new(e)),
            }
        }
    }
}

/// Configuration for test database
#[derive(Clone, Debug)]
pub struct TestDatabaseConfig {
    pub db_name: String,
    pub user: String,
    pub password: String,
}

impl Default for TestDatabaseConfig {
    fn default() -> Self {
        TestDatabaseConfig {
            db_name: "test_db".to_string(),
            user: "postgres".to_string(),
            password: "postgres".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_connection() {
        let db = TestDatabase::new().await.expect("Failed to create test database");
        let _client = db.client().await.expect("Failed to get client");
        // Database will be cleaned up when db is dropped
    }

    #[tokio::test]
    async fn test_database_query() {
        let db = TestDatabase::new().await.expect("Failed to create test database");
        let rows = db.query("SELECT 1 as num", &[])
            .await
            .expect("Query failed");
        assert_eq!(rows.len(), 1);
    }
}
```

---

### Step 4: Create Test Fixtures

**File**: `fraiseql_rs/tests/common/fixtures.rs`

```rust
//! Test fixtures and sample data
//!
//! Provides pre-built test data for consistent testing across phases

use serde_json::{json, Value};

/// Sample table schema for testing
pub struct SampleSchema;

impl SampleSchema {
    /// Create users table for testing
    pub fn users_table_sql() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email VARCHAR(255) UNIQUE NOT NULL,
            age INT,
            is_active BOOLEAN DEFAULT true,
            metadata JSONB DEFAULT '{}',
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#
    }

    /// Create posts table for testing
    pub fn posts_table_sql() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS posts (
            id SERIAL PRIMARY KEY,
            user_id INT REFERENCES users(id),
            title VARCHAR(255) NOT NULL,
            content TEXT,
            tags JSONB DEFAULT '[]',
            published BOOLEAN DEFAULT false,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#
    }

    /// Create products table for testing (with complex JSONB)
    pub fn products_table_sql() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS products (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            price DECIMAL(10, 2),
            attributes JSONB DEFAULT '{}',
            inventory JSONB DEFAULT '{"stock": 0}',
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#
    }
}

/// Sample data for testing
pub struct SampleData;

impl SampleData {
    /// Insert sample users
    pub fn insert_users_sql() -> &'static str {
        r#"
        INSERT INTO users (name, email, age, metadata)
        VALUES
            ('Alice', 'alice@example.com', 30, '{"role": "admin"}'),
            ('Bob', 'bob@example.com', 25, '{"role": "user"}'),
            ('Charlie', 'charlie@example.com', 35, '{"role": "user", "verified": true}')
        ON CONFLICT DO NOTHING;
        "#
    }

    /// Insert sample posts
    pub fn insert_posts_sql() -> &'static str {
        r#"
        INSERT INTO posts (user_id, title, content, tags, published)
        VALUES
            (1, 'First Post', 'Hello World', '["rust", "postgres"]', true),
            (1, 'Second Post', 'Async Rust', '["async", "rust"]', true),
            (2, 'Draft Post', 'Work in progress', '["draft"]', false)
        ON CONFLICT DO NOTHING;
        "#
    }

    /// Insert sample products
    pub fn insert_products_sql() -> &'static str {
        r#"
        INSERT INTO products (name, price, attributes, inventory)
        VALUES
            ('Laptop', 999.99, '{"brand": "Dell", "specs": {"cpu": "i7", "ram": "16GB"}}', '{"stock": 5, "warehouse": "A"}'),
            ('Mouse', 29.99, '{"brand": "Logitech", "color": "black"}', '{"stock": 50, "warehouse": "B"}'),
            ('Keyboard', 79.99, '{"brand": "Mechanical", "switches": "Blue"}', '{"stock": 0, "warehouse": "C"}')
        ON CONFLICT DO NOTHING;
        "#
    }
}

/// JSON value builders for WHERE clause testing
pub struct JsonTestValues;

impl JsonTestValues {
    pub fn simple_object() -> Value {
        json!({"key": "value", "number": 42})
    }

    pub fn nested_object() -> Value {
        json!({
            "user": {
                "name": "Alice",
                "contact": {
                    "email": "alice@example.com",
                    "phone": "+1-555-0123"
                }
            }
        })
    }

    pub fn array_value() -> Value {
        json!(["item1", "item2", "item3"])
    }

    pub fn mixed_types() -> Value {
        json!({
            "string": "text",
            "number": 123,
            "boolean": true,
            "null": null,
            "array": [1, 2, 3],
            "object": {"nested": "value"}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_schema_valid() {
        let sql = SampleSchema::users_table_sql();
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn test_json_test_values() {
        let obj = JsonTestValues::simple_object();
        assert!(obj.get("key").is_some());
        assert_eq!(obj.get("number").and_then(|v| v.as_i64()), Some(42));
    }
}
```

---

### Step 5: Create Custom Assertions

**File**: `fraiseql_rs/tests/common/assertions.rs`

```rust
//! Custom assertions for PostgreSQL and JSON testing

/// Assert that a SQL query result contains expected rows
#[macro_export]
macro_rules! assert_query_rows {
    ($result:expr, $expected:expr) => {
        assert_eq!(
            $result.len(),
            $expected,
            "Expected {} rows, got {}",
            $expected,
            $result.len()
        )
    };
}

/// Assert that a JSON value matches expected structure
#[macro_export]
macro_rules! assert_json_matches {
    ($actual:expr, $expected:expr) => {
        let actual_str = $actual.to_string();
        let expected_str = $expected.to_string();
        assert_eq!(
            actual_str, expected_str,
            "JSON mismatch:\nExpected: {}\nActual: {}",
            expected_str, actual_str
        )
    };
}

/// Assert that a WHERE clause generates correct SQL
#[macro_export]
macro_rules! assert_where_sql {
    ($where_clause:expr, $expected_sql:expr) => {
        assert_eq!(
            $where_clause.to_sql(),
            $expected_sql,
            "WHERE clause SQL mismatch"
        )
    };
}

/// Assert that a column value matches expected type and value
#[macro_export]
macro_rules! assert_column_value {
    ($row:expr, $col_name:expr, $expected:expr) => {
        let value: &(dyn std::any::Any) = &$row.try_get::<_, i64>($col_name).unwrap();
        assert_eq!(
            std::any::TypeId::of_val(value),
            std::any::TypeId::of($expected),
            "Type mismatch for column {}: expected {}, got {}",
            $col_name,
            std::any::type_name_of_val(&$expected),
            std::any::type_name_of_val(value)
        )
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_custom_macros_compile() {
        // These macros are tested by compilation
        // If they compile, they work
    }
}
```

---

### Step 6: Add Test Dependencies to Cargo.toml

**File**: `fraiseql_rs/Cargo.toml` (add to `[dev-dependencies]`)

```toml
[dev-dependencies]
# Testing framework
tokio-test = "0.4"                    # Async runtime for tests
tokio = { version = "1.0", features = ["full"] }

# Test database containers
testcontainers = "0.15"               # Docker containers for tests
testcontainers-modules = { version = "0.2", features = ["postgres"] }

# Assertions and matchers
assert_matches = "1.5"                # Pattern matching in assertions
pretty_assertions = "1.4"             # Pretty-print assertion failures

# Mocking
mockall = "0.12"                      # Mock objects for unit tests

# Property testing
proptest = "1.3"                      # Generate test cases

# Benchmarking (covered in Phase 0.3)
criterion = { version = "0.5", features = ["async_tokio"] }

# JSON testing
serde_json = "1.0"
```

---

### Step 7: Configure Test Execution

**File**: `Cargo.toml` (add to `[profile.test]`)

```toml
# Test profile configuration
[profile.test]
opt-level = 1                    # Some optimization for faster tests
incremental = true               # Faster rebuild during test development

# Keep debug info for better error messages
debug = true
debug-assertions = true
overflow-checks = true
```

---

### Step 8: Create Makefile Test Targets

**File**: `Makefile` (add test targets)

```makefile
# ============================================================================
# Testing Targets
# ============================================================================

.PHONY: test test-unit test-integration test-all test-verbose coverage

## test: Run full test suite (unit + integration)
test:
 cd fraiseql_rs && cargo test --lib --test '*'
 @echo "✅ All tests passed"

## test-unit: Run only unit tests (fast)
test-unit:
 cd fraiseql_rs && cargo test --lib
 @echo "✅ Unit tests passed"

## test-integration: Run only integration tests (requires DB)
test-integration:
 cd fraiseql_rs && cargo test --test '*'
 @echo "✅ Integration tests passed"

## test-all: Run all tests including e2e and examples
test-all: test
 cd fraiseql_rs && cargo test --all
 @echo "✅ All tests passed including examples"

## test-verbose: Run tests with verbose output
test-verbose:
 cd fraiseql_rs && cargo test --all -- --nocapture --test-threads=1
 @echo "✅ Verbose test run complete"

## coverage: Generate code coverage report
coverage:
 cd fraiseql_rs && cargo tarpaulin --out Html --output-dir coverage/
 @echo "📊 Coverage report generated in coverage/index.html"

## watch: Watch for changes and run tests (requires cargo-watch)
watch:
 cargo watch -x "test --lib" -x clippy
```

---

### Step 9: Verify Setup

**Commands to run**:

```bash
# 1. Check test structure
ls -la fraiseql_rs/tests/
# Expected: common/, unit/, integration/, e2e/ directories

# 2. Compile tests (no execution needed yet)
cd fraiseql_rs && cargo test --no-run
# Expected: Compilation succeeds

# 3. Run quick unit test
cd fraiseql_rs && cargo test --lib
# Expected: At least one test passes

# 4. Verify Makefile targets
make test-unit
make test-all
# Expected: Both targets work
```

---

## Troubleshooting

### "Docker not found" error

**Issue**: testcontainers can't start PostgreSQL container

**Fix**:

```bash
# Install Docker
# macOS
brew install docker
colima start  # Start Docker daemon

# Linux
sudo apt-get install docker.io
sudo usermod -aG docker $USER
```

---

### "Connection refused" in integration tests

**Issue**: Test database not ready in time

**Fix**: Increase retry timeout in `TestDatabase::wait_for_db()`:

```rust
let mut retries = 60;  // Increased from 30
tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;  // Increased from 100
```

---

## Success Criteria

- ✅ Test directory structure created
- ✅ TestDatabase helper working
- ✅ Sample schemas and data available
- ✅ Custom assertions compile
- ✅ At least one unit test passing
- ✅ At least one integration test passing
- ✅ Makefile targets functional

---

## Next Steps

1. Commit test infrastructure
2. Run `make test` to verify setup
3. Move to Phase 0.3 (Benchmarks)

---

## 👥 Review Checkpoint for Junior Engineers

**After completing Phase 0.2, ask a senior developer to review**:

- [ ] Test directory structure looks reasonable?
- [ ] TestDatabase implementation follows best practices?
- [ ] Docker container management is correct?
- [ ] Async test setup looks good?

**Why**: Test infrastructure is foundational. Getting feedback now prevents problems in all future phases.

**What to prepare for review**:

```bash
# Show your work
git add fraiseql_rs/tests/
git status  # Show all test files

# Run tests so reviewer can verify
cargo test --lib
```

---

**Estimated Duration**: 1.5 hours

- Create directories: 10 min
- Write TestDatabase: 30 min
- Write fixtures and assertions: 30 min
- Configure Cargo.toml: 15 min
- Verify setup: 15 min

**Last Updated**: 2025-12-18
