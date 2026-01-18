# FraiseQL-Wire Integration Implementation Plan

**Date**: 2026-01-13
**Status**: Planning Phase
**Goal**: Integrate fraiseql-wire as dual data plane backend for FraiseQL v2
**Total Estimated Effort**: 4-6 weeks

---

## Overview

This plan implements a **dual data plane architecture** for FraiseQL v2:
- **fraiseql-wire**: Standard GraphQL queries (memory-efficient streaming)
- **Arrow + Polars**: Analytics queries (high-performance aggregations)
- **Unified Router**: Transparent query routing in `Executor`

---

## Phase 0: Prerequisites & Foundation

**Goal**: Fix critical build errors and establish baseline metrics.

**Estimated Effort**: 2-3 days

### Tasks

#### 0.1: Fix Build Errors (CRITICAL)

**Issue**: Tests failing due to missing fields in schema structs.

**Files**:
- `crates/fraiseql-core/src/schema/compiled.rs`
- `crates/fraiseql-core/tests/integration/schema_test.rs`

**Steps**:
1. Add missing `fact_tables` field to `CompiledSchema`
2. Add missing `calendar_dimensions` field to `CompiledSchema`
3. Update all schema construction in tests
4. Verify all tests compile and pass

**Verification**:
```bash
cargo check --all-targets
cargo clippy --all-targets --all-features
cargo nextest run
```

**Expected Output**:
```
✅ All checks pass
✅ All clippy lints pass
✅ All tests pass (0 failures)
```

**Acceptance Criteria**:
- [ ] `cargo check` passes with zero errors
- [ ] `cargo clippy` passes with zero warnings
- [ ] `cargo nextest run` shows 100% pass rate
- [ ] No TODO comments about missing fields

---

#### 0.2: Establish Performance Baseline

**Goal**: Measure current memory usage and latency for comparison.

**Steps**:
1. Create benchmark suite for standard queries
2. Measure memory usage for 10K, 100K, 1M row queries
3. Measure time-to-first-row latency
4. Document baseline metrics

**Create Benchmark** (`benches/database_baseline.rs`):
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fraiseql_core::db::PostgresAdapter;

fn benchmark_large_query(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let adapter = rt.block_on(async {
        PostgresAdapter::new("postgresql://localhost/test").await.unwrap()
    });

    c.bench_function("query_100k_rows", |b| {
        b.to_async(&rt).iter(|| async {
            let results = adapter
                .execute_where_query("v_user", None, Some(100_000), None)
                .await
                .unwrap();
            black_box(results);
        });
    });
}

criterion_group!(benches, benchmark_large_query);
criterion_main!(benches);
```

**Run Benchmarks**:
```bash
# Memory profiling
cargo build --release --benches
heaptrack target/release/deps/database_baseline-*

# Latency benchmarking
cargo bench --bench database_baseline
```

**Document Results** (`.claude/analysis/baseline-metrics.md`):
```markdown
## Baseline Metrics (tokio-postgres)

| Query Size | Memory Usage | Time-to-First-Row | Throughput |
|-----------|--------------|-------------------|------------|
| 10K rows  | 2.6 MB       | 2.3 ms            | 450K rows/s |
| 100K rows | 26 MB        | 2.5 ms            | 480K rows/s |
| 1M rows   | 260 MB       | 2.8 ms            | 420K rows/s |
```

**Acceptance Criteria**:
- [ ] Benchmark suite created in `benches/`
- [ ] Memory measurements documented for 10K, 100K, 1M rows
- [ ] Latency measurements documented
- [ ] Baseline metrics file committed

---

## Phase 1: fraiseql-wire Adapter Implementation

**Goal**: Implement `FraiseWireAdapter` as drop-in replacement for `PostgresAdapter`.

**Estimated Effort**: 3-4 days

### Tasks

#### 1.1: Add Dependencies

**File**: `crates/fraiseql-core/Cargo.toml`

**Changes**:
```toml
[dependencies]
# Existing dependencies...
fraiseql-wire = { path = "../../../fraiseql-wire", optional = true }
deadpool = { version = "0.10", optional = true }

[features]
default = ["postgres"]
postgres = ["sqlx/postgres", "tokio-postgres"]
wire-backend = ["fraiseql-wire", "deadpool"]  # NEW
```

**Verification**:
```bash
cargo check --features wire-backend
```

**Acceptance Criteria**:
- [ ] fraiseql-wire added as optional dependency
- [ ] deadpool added for connection pooling
- [ ] `wire-backend` feature compiles successfully

---

#### 1.2: Implement WHERE Clause Generator

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs` (NEW)

**Purpose**: Convert `WhereClause` AST to SQL string for fraiseql-wire.

**Implementation**:
```rust
//! WHERE clause to SQL string generator.
//!
//! Converts FraiseQL's WHERE clause AST to SQL predicates for fraiseql-wire.

use crate::db::{WhereClause, WhereOperator};
use crate::error::{FraiseQLError, Result};
use serde_json::Value;

/// Generates SQL WHERE clause strings from AST.
pub struct WhereSqlGenerator;

impl WhereSqlGenerator {
    /// Convert WHERE clause AST to SQL string.
    ///
    /// # Example
    ///
    /// ```rust
    /// let clause = WhereClause::Field {
    ///     path: vec!["status".to_string()],
    ///     operator: WhereOperator::Eq,
    ///     value: json!("active"),
    /// };
    ///
    /// let sql = WhereSqlGenerator::to_sql(&clause)?;
    /// assert_eq!(sql, "data->>'status' = 'active'");
    /// ```
    pub fn to_sql(clause: &WhereClause) -> Result<String> {
        match clause {
            WhereClause::Field { path, operator, value } => {
                Self::generate_field_predicate(path, operator, value)
            }
            WhereClause::And(clauses) => {
                let parts: Result<Vec<_>> = clauses
                    .iter()
                    .map(Self::to_sql)
                    .collect();
                Ok(format!("({})", parts?.join(" AND ")))
            }
            WhereClause::Or(clauses) => {
                let parts: Result<Vec<_>> = clauses
                    .iter()
                    .map(Self::to_sql)
                    .collect();
                Ok(format!("({})", parts?.join(" OR ")))
            }
            WhereClause::Not(clause) => {
                let inner = Self::to_sql(clause)?;
                Ok(format!("NOT ({})", inner))
            }
        }
    }

    fn generate_field_predicate(
        path: &[String],
        operator: &WhereOperator,
        value: &Value,
    ) -> Result<String> {
        let json_path = Self::build_json_path(path);
        let sql_op = Self::operator_to_sql(operator, value)?;
        let sql_value = Self::value_to_sql(value, operator)?;

        Ok(format!("{} {} {}", json_path, sql_op, sql_value))
    }

    fn build_json_path(path: &[String]) -> String {
        if path.is_empty() {
            return "data".to_string();
        }

        if path.len() == 1 {
            // Simple path: data->>'field'
            format!("data->>'{}', path[0])
        } else {
            // Nested path: data#>'{a,b,c}'->>'d'
            let nested = &path[..path.len() - 1];
            let last = &path[path.len() - 1];
            let nested_path = nested.join(",");
            format!("data#>'{{{}}}'->>'{}', nested_path, last)
        }
    }

    fn operator_to_sql(operator: &WhereOperator, value: &Value) -> Result<&'static str> {
        Ok(match operator {
            WhereOperator::Eq => "=",
            WhereOperator::Ne => "!=",
            WhereOperator::Gt => ">",
            WhereOperator::Gte => ">=",
            WhereOperator::Lt => "<",
            WhereOperator::Lte => "<=",
            WhereOperator::Contains => "LIKE",
            WhereOperator::Icontains => "ILIKE",
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Endswith => "LIKE",
            WhereOperator::In => "= ANY",
            WhereOperator::IsNull => "IS NULL",
            WhereOperator::IsNotNull => "IS NOT NULL",
        })
    }

    fn value_to_sql(value: &Value, operator: &WhereOperator) -> Result<String> {
        match (value, operator) {
            (Value::Null, _) => Ok("NULL".to_string()),
            (Value::Bool(b), _) => Ok(b.to_string()),
            (Value::Number(n), _) => Ok(n.to_string()),
            (Value::String(s), WhereOperator::Contains | WhereOperator::Icontains) => {
                Ok(format!("'%{}%'", Self::escape_sql_string(s)))
            }
            (Value::String(s), WhereOperator::Startswith) => {
                Ok(format!("'{}%'", Self::escape_sql_string(s)))
            }
            (Value::String(s), WhereOperator::Endswith) => {
                Ok(format!("'%{}'", Self::escape_sql_string(s)))
            }
            (Value::String(s), _) => {
                Ok(format!("'{}'", Self::escape_sql_string(s)))
            }
            (Value::Array(arr), WhereOperator::In) => {
                let values: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| Self::value_to_sql(v, &WhereOperator::Eq))
                    .collect();
                Ok(format!("ARRAY[{}]", values?.join(", ")))
            }
            _ => Err(FraiseQLError::Internal(
                format!("Unsupported value type for operator: {:?}", operator)
            )),
        }
    }

    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_equality() {
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'status' = 'active'");
    }

    #[test]
    fn test_nested_path() {
        let clause = WhereClause::Field {
            path: vec!["user".to_string(), "email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("test@example.com"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data#>'{user}'->>'email' = 'test@example.com'");
    }

    #[test]
    fn test_icontains() {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value: json!("john"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'name' ILIKE '%john%'");
    }

    #[test]
    fn test_and_clause() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value: json!("active"),
            },
            WhereClause::Field {
                path: vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value: json!(18),
            },
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "(data->>'status' = 'active' AND data->>'age' >= 18)");
    }

    #[test]
    fn test_sql_injection_prevention() {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Eq,
            value: json!("'; DROP TABLE users; --"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'name' = '''; DROP TABLE users; --'");
    }
}
```

**Verification**:
```bash
cargo test --lib where_sql_generator
```

**Acceptance Criteria**:
- [ ] All WHERE operators supported (Eq, Ne, Gt, Lt, Contains, etc.)
- [ ] Nested JSON paths work correctly
- [ ] SQL injection protection (string escaping)
- [ ] Unit tests for all operators and edge cases
- [ ] Tests pass with 100% coverage

---

#### 1.3: Implement Connection Pool

**File**: `crates/fraiseql-core/src/db/wire_pool.rs` (NEW)

**Purpose**: Pool of `FraiseClient` connections for reuse.

**Implementation**:
```rust
//! Connection pool for fraiseql-wire clients.

use deadpool::managed::{Manager, Pool, PoolError, RecycleResult};
use fraiseql_wire::FraiseClient;
use std::sync::Arc;

/// Manager for fraiseql-wire client pool.
pub struct FraiseClientManager {
    connection_string: String,
}

impl FraiseClientManager {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait::async_trait]
impl Manager for FraiseClientManager {
    type Type = FraiseClient;
    type Error = fraiseql_wire::Error;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        FraiseClient::connect(&self.connection_string).await
    }

    async fn recycle(
        &self,
        client: &mut Self::Type,
        _metrics: &deadpool::managed::Metrics,
    ) -> RecycleResult<Self::Error> {
        // Health check: verify connection is still alive
        // fraiseql-wire doesn't have built-in health check yet,
        // so we'll assume connection is valid
        // TODO: Add ping query once fraiseql-wire supports it
        Ok(())
    }
}

/// Connection pool for FraiseClient.
pub type FraiseClientPool = Pool<FraiseClientManager>;

/// Create a new connection pool.
pub fn create_pool(connection_string: &str, max_size: usize) -> FraiseClientPool {
    let manager = FraiseClientManager::new(connection_string.to_string());
    Pool::builder(manager)
        .max_size(max_size)
        .build()
        .expect("Failed to create connection pool")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_creation() {
        let pool = create_pool("postgres://localhost/test", 4);
        assert_eq!(pool.status().max_size, 4);
    }

    #[tokio::test]
    async fn test_pool_acquire_release() {
        let pool = create_pool("postgres://localhost/test", 2);

        // Acquire two connections
        let conn1 = pool.get().await.unwrap();
        let conn2 = pool.get().await.unwrap();

        // Drop first connection (returns to pool)
        drop(conn1);

        // Should be able to acquire again
        let conn3 = pool.get().await.unwrap();
        assert!(conn3.is_ok());
    }
}
```

**Acceptance Criteria**:
- [ ] Pool creates connections on demand
- [ ] Pool reuses connections efficiently
- [ ] Pool respects max_size configuration
- [ ] Tests verify acquire/release semantics

---

#### 1.4: Implement FraiseWireAdapter

**File**: `crates/fraiseql-core/src/db/fraiseql_wire_adapter.rs` (NEW)

**Purpose**: Implement `DatabaseAdapter` trait using fraiseql-wire.

**Implementation**:
```rust
//! FraiseQL-Wire database adapter.
//!
//! This adapter uses fraiseql-wire for memory-efficient streaming of query results.

use async_trait::async_trait;
use futures::StreamExt;

use crate::db::{
    DatabaseAdapter, DatabaseCapabilities, DatabaseType, JsonbValue, PoolMetrics, WhereClause,
};
use crate::error::{FraiseQLError, Result};

use super::where_sql_generator::WhereSqlGenerator;
use super::wire_pool::{create_pool, FraiseClientPool};

/// Database adapter using fraiseql-wire for streaming queries.
pub struct FraiseWireAdapter {
    pool: FraiseClientPool,
    connection_string: String,
}

impl FraiseWireAdapter {
    /// Create new adapter with connection pooling.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - Postgres connection string
    /// * `pool_size` - Maximum number of connections (default: 16)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = FraiseWireAdapter::new("postgres://localhost/mydb", 16).await?;
    /// ```
    pub async fn new(connection_string: &str, pool_size: usize) -> Result<Self> {
        let pool = create_pool(connection_string, pool_size);

        // Test connection
        let client = pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool(format!("Failed to acquire connection: {}", e))
        })?;

        // TODO: Add health check once fraiseql-wire supports it
        drop(client);

        Ok(Self {
            pool,
            connection_string: connection_string.to_string(),
        })
    }

    /// Create adapter with default pool size (16 connections).
    pub async fn with_defaults(connection_string: &str) -> Result<Self> {
        Self::new(connection_string, 16).await
    }
}

#[async_trait]
impl DatabaseAdapter for FraiseWireAdapter {
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Acquire connection from pool
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool(format!("Pool exhausted: {}", e))
        })?;

        // Build query
        let mut query = client.query(view);

        // Add WHERE clause
        if let Some(clause) = where_clause {
            let sql = WhereSqlGenerator::to_sql(clause)?;
            query = query.where_sql(&sql);
        }

        // Execute query and get stream
        let mut stream = query.execute().await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Query execution failed: {}", e),
                code: None,
            }
        })?;

        // Collect results with limit/offset
        let mut results = Vec::new();
        let mut count = 0;
        let skip = offset.unwrap_or(0);
        let take = limit.unwrap_or(u32::MAX);

        while let Some(item) = stream.next().await {
            let json = item.map_err(|e| FraiseQLError::Database {
                message: format!("Stream error: {}", e),
                code: None,
            })?;

            // Skip offset rows
            if count < skip {
                count += 1;
                continue;
            }

            // Stop at limit
            if results.len() >= take as usize {
                break;
            }

            results.push(JsonbValue(json));
            count += 1;
        }

        Ok(results)
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool(format!("Health check failed: {}", e))
        })?;

        // Execute simple query
        let mut stream = client
            .query("pg_catalog.pg_type")
            .where_sql("oid = 16") // boolean type
            .execute()
            .await
            .map_err(|e| FraiseQLError::Database {
                message: format!("Health check query failed: {}", e),
                code: None,
            })?;

        // Check we got at least one result
        stream.next().await.ok_or_else(|| {
            FraiseQLError::Database {
                message: "Health check returned no results".to_string(),
                code: None,
            }
        })??;

        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        let status = self.pool.status();
        PoolMetrics {
            total: status.max_size,
            idle: status.available,
            active: status.size - status.available,
            waiting: status.waiting,
        }
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities {
            supports_jsonb: true,
            supports_aggregates: true,
            supports_window_functions: true,
            supports_materialized_views: true,
            max_parameters: 65535, // Postgres limit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::WhereOperator;
    use serde_json::json;

    // Note: These tests require a running Postgres instance
    // Run with: cargo test --features wire-backend -- --ignored

    #[tokio::test]
    #[ignore]
    async fn test_adapter_creation() {
        let adapter = FraiseWireAdapter::with_defaults("postgres://localhost/test")
            .await
            .unwrap();

        assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
    }

    #[tokio::test]
    #[ignore]
    async fn test_simple_query() {
        let adapter = FraiseWireAdapter::with_defaults("postgres://localhost/test")
            .await
            .unwrap();

        let results = adapter
            .execute_where_query("v_user", None, Some(10), None)
            .await
            .unwrap();

        assert!(results.len() <= 10);
    }

    #[tokio::test]
    #[ignore]
    async fn test_where_clause() {
        let adapter = FraiseWireAdapter::with_defaults("postgres://localhost/test")
            .await
            .unwrap();

        let where_clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();

        // Verify all results match filter
        for result in results {
            let status = result.0["status"].as_str().unwrap();
            assert_eq!(status, "active");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_pagination() {
        let adapter = FraiseWireAdapter::with_defaults("postgres://localhost/test")
            .await
            .unwrap();

        // Get first page
        let page1 = adapter
            .execute_where_query("v_user", None, Some(10), Some(0))
            .await
            .unwrap();

        // Get second page
        let page2 = adapter
            .execute_where_query("v_user", None, Some(10), Some(10))
            .await
            .unwrap();

        // Pages should not overlap
        assert!(page1.len() <= 10);
        assert!(page2.len() <= 10);
    }

    #[tokio::test]
    #[ignore]
    async fn test_health_check() {
        let adapter = FraiseWireAdapter::with_defaults("postgres://localhost/test")
            .await
            .unwrap();

        adapter.health_check().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_pool_metrics() {
        let adapter = FraiseWireAdapter::new("postgres://localhost/test", 4)
            .await
            .unwrap();

        let metrics = adapter.pool_metrics();
        assert_eq!(metrics.total, 4);
        assert!(metrics.idle <= 4);
    }
}
```

**Acceptance Criteria**:
- [ ] Implements all `DatabaseAdapter` methods
- [ ] WHERE clause translation works correctly
- [ ] Pagination (limit/offset) applied correctly
- [ ] Health check verifies connectivity
- [ ] Pool metrics exposed
- [ ] Integration tests pass (with test database)

---

#### 1.5: Update Module Exports

**File**: `crates/fraiseql-core/src/db/mod.rs`

**Changes**:
```rust
pub mod collation;
pub mod traits;
pub mod types;
pub mod where_clause;
pub mod where_sql_generator;  // NEW

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "wire-backend")]
pub mod wire_pool;  // NEW

#[cfg(feature = "wire-backend")]
pub mod fraiseql_wire_adapter;  // NEW

// Re-export
pub use where_sql_generator::WhereSqlGenerator;  // NEW

#[cfg(feature = "wire-backend")]
pub use fraiseql_wire_adapter::FraiseWireAdapter;  // NEW
```

**Acceptance Criteria**:
- [ ] New modules exported correctly
- [ ] Feature flags respected
- [ ] No compilation errors

---

#### 1.6: Add Comparison Benchmarks

**File**: `benches/adapter_comparison.rs` (NEW)

**Purpose**: Compare tokio-postgres vs fraiseql-wire memory usage and performance.

**Implementation**:
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use fraiseql_core::db::{DatabaseAdapter, PostgresAdapter};

#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;

fn benchmark_adapters(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup adapters
    let postgres_adapter = rt.block_on(async {
        PostgresAdapter::new("postgresql://localhost/test").await.unwrap()
    });

    #[cfg(feature = "wire-backend")]
    let wire_adapter = rt.block_on(async {
        FraiseWireAdapter::with_defaults("postgres://localhost/test").await.unwrap()
    });

    let sizes = vec![1_000, 10_000, 100_000];

    for size in sizes {
        // Benchmark tokio-postgres
        c.bench_with_input(
            BenchmarkId::new("postgres", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let results = postgres_adapter
                        .execute_where_query("v_user", None, Some(size), None)
                        .await
                        .unwrap();
                    black_box(results);
                });
            },
        );

        // Benchmark fraiseql-wire
        #[cfg(feature = "wire-backend")]
        c.bench_with_input(
            BenchmarkId::new("wire", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let results = wire_adapter
                        .execute_where_query("v_user", None, Some(size), None)
                        .await
                        .unwrap();
                    black_box(results);
                });
            },
        );
    }
}

criterion_group!(benches, benchmark_adapters);
criterion_main!(benches);
```

**Run Benchmarks**:
```bash
# With memory profiling
cargo build --release --features wire-backend --benches
heaptrack target/release/deps/adapter_comparison-*

# Performance comparison
cargo bench --features wire-backend --bench adapter_comparison
```

**Acceptance Criteria**:
- [ ] Benchmarks run successfully for both adapters
- [ ] Memory usage shows significant reduction for wire adapter
- [ ] Latency is comparable between adapters
- [ ] Results documented in `.claude/analysis/`

---

## Phase 2: Arrow Analytics Backend

**Goal**: Implement Arrow/Polars backend for analytics queries.

**Estimated Effort**: 5-7 days

### Tasks

#### 2.1: Add Arrow Dependencies

**File**: `crates/fraiseql-core/Cargo.toml`

**Changes**:
```toml
[dependencies]
# Existing...
arrow = { version = "53", optional = true }
polars = { version = "0.42", features = ["lazy", "sql", "parquet"], optional = true }
datafusion = { version = "42", optional = true }

[features]
analytics = ["arrow", "polars", "datafusion"]
```

**Acceptance Criteria**:
- [ ] Arrow/Polars dependencies added
- [ ] `analytics` feature compiles successfully

---

#### 2.2: Implement Arrow Adapter

**File**: `crates/fraiseql-core/src/db/arrow_adapter.rs` (NEW)

**Purpose**: Execute analytics queries using Arrow/Polars.

**Implementation Outline**:
```rust
use arrow::record_batch::RecordBatch;
use polars::prelude::*;
use crate::error::Result;

pub struct ArrowAnalyticsAdapter {
    postgres_pool: PgPool,  // For initial data loading
    cache: Option<Arc<QueryCache>>,  // Cache intermediate results
}

impl ArrowAnalyticsAdapter {
    pub async fn execute_aggregate(
        &self,
        fact_table: &str,
        dimensions: &[String],
        measures: &[MeasureDef],
        filters: Option<&WhereClause>,
    ) -> Result<RecordBatch> {
        // 1. Load data from Postgres
        let df = self.load_fact_table(fact_table, filters).await?;

        // 2. Apply aggregations
        let result = df
            .lazy()
            .groupby(dimensions.iter().map(|d| col(d)))
            .agg(measures.iter().map(|m| m.to_polars_expr()).collect())
            .collect()?;

        // 3. Convert to Arrow RecordBatch
        Ok(result.to_arrow()?)
    }

    pub async fn execute_window(
        &self,
        fact_table: &str,
        partition_by: &[String],
        order_by: &[String],
        window_fn: &WindowFunction,
    ) -> Result<RecordBatch> {
        // Similar pattern for window functions
        todo!("Implement in Phase 2.2")
    }

    async fn load_fact_table(
        &self,
        table: &str,
        filters: Option<&WhereClause>,
    ) -> Result<DataFrame> {
        // Load data from Postgres into Polars DataFrame
        todo!("Implement in Phase 2.2")
    }
}
```

**Acceptance Criteria**:
- [ ] Can load fact tables from Postgres
- [ ] Aggregations execute using Polars
- [ ] Window functions supported
- [ ] Results convert to Arrow RecordBatch
- [ ] Integration tests pass

---

#### 2.3: Integrate with Executor

**File**: `crates/fraiseql-core/src/runtime/executor.rs`

**Changes**:
```rust
pub struct Executor {
    schema: CompiledSchema,

    // Dual data plane
    #[cfg(feature = "wire-backend")]
    standard_adapter: Arc<FraiseWireAdapter>,

    #[cfg(feature = "analytics")]
    analytics_adapter: Arc<ArrowAnalyticsAdapter>,

    // Fallback to postgres
    #[cfg(not(feature = "wire-backend"))]
    standard_adapter: Arc<PostgresAdapter>,

    matcher: QueryMatcher,
    planner: QueryPlanner,
    config: RuntimeConfig,
}

impl Executor {
    #[cfg(all(feature = "wire-backend", feature = "analytics"))]
    pub async fn new_dual_plane(
        schema: CompiledSchema,
        standard_conn: &str,
        analytics_conn: &str,
    ) -> Result<Self> {
        let standard_adapter = Arc::new(
            FraiseWireAdapter::with_defaults(standard_conn).await?
        );
        let analytics_adapter = Arc::new(
            ArrowAnalyticsAdapter::new(analytics_conn).await?
        );

        Ok(Self {
            schema: schema.clone(),
            standard_adapter,
            analytics_adapter,
            matcher: QueryMatcher::new(schema.clone()),
            planner: QueryPlanner::new(true),
            config: RuntimeConfig::default(),
        })
    }

    async fn execute_aggregate_with_arrow(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        #[cfg(feature = "analytics")]
        {
            // Extract aggregate query definition
            let agg_def = self.schema.aggregate_queries
                .get(query_name)
                .ok_or_else(|| FraiseQLError::QueryNotFound(query_name.to_string()))?;

            // Execute using Arrow adapter
            let batch = self.analytics_adapter
                .execute_aggregate(
                    &agg_def.fact_table,
                    &agg_def.dimensions,
                    &agg_def.measures,
                    None,  // TODO: parse filters from variables
                )
                .await?;

            // Convert RecordBatch to GraphQL JSON
            let json = self.batch_to_graphql_json(&batch, query_name)?;
            Ok(json)
        }

        #[cfg(not(feature = "analytics"))]
        {
            // Fallback to SQL-based execution
            self.execute_aggregate_dispatch(query_name, variables).await
        }
    }
}
```

**Acceptance Criteria**:
- [ ] Query router selects correct backend
- [ ] Standard queries use fraiseql-wire
- [ ] Analytics queries use Arrow/Polars
- [ ] Fallback to SQL works without features
- [ ] All existing tests pass

---

## Phase 3: Testing & Validation

**Goal**: Comprehensive testing of dual backend system.

**Estimated Effort**: 2-3 days

### Tasks

#### 3.1: Integration Test Suite

**File**: `crates/fraiseql-core/tests/integration/dual_backend_test.rs` (NEW)

**Tests**:
```rust
#[tokio::test]
#[cfg(feature = "wire-backend")]
async fn test_standard_query_uses_wire_backend() {
    // Setup executor with dual backends
    let executor = Executor::new_dual_plane(...).await.unwrap();

    // Execute standard query
    let query = r#"query { users(limit: 100) { id name } }"#;
    let result = executor.execute(query, None).await.unwrap();

    // Verify result structure
    assert!(result.contains("users"));

    // TODO: Add telemetry to verify wire backend was used
}

#[tokio::test]
#[cfg(feature = "analytics")]
async fn test_aggregate_query_uses_arrow_backend() {
    let executor = Executor::new_dual_plane(...).await.unwrap();

    let query = r#"query { sales_aggregate { ... } }"#;
    let result = executor.execute(query, None).await.unwrap();

    // Verify Arrow backend was used (via telemetry)
    assert!(result.contains("sales_aggregate"));
}

#[tokio::test]
async fn test_memory_efficiency() {
    // Compare memory usage between backends
    // Use memory profiling tools
}

#[tokio::test]
async fn test_concurrent_queries() {
    // Stress test connection pools
    // Multiple concurrent queries
}
```

**Acceptance Criteria**:
- [ ] Standard queries route to wire backend
- [ ] Analytics queries route to Arrow backend
- [ ] Memory usage validated
- [ ] Concurrent queries handled correctly
- [ ] Error handling tested

---

#### 3.2: Performance Regression Tests

**File**: `tests/performance/regression_tests.rs` (NEW)

**Tests**:
- Compare latency before/after integration
- Measure memory usage reduction
- Verify throughput maintained
- Check connection pool efficiency

**Acceptance Criteria**:
- [ ] No latency regression for standard queries
- [ ] Memory usage reduced as expected (20,000x for 100K rows)
- [ ] Throughput matches baseline
- [ ] Connection pools scale correctly

---

#### 3.3: Update Documentation

**Files to Update**:
- `README.md` - Add dual backend architecture section
- `docs/architecture.md` - Explain query routing
- `docs/performance.md` - Document memory improvements
- `.claude/CLAUDE.md` - Update implementation status

**Acceptance Criteria**:
- [ ] Architecture documented clearly
- [ ] Performance characteristics explained
- [ ] Examples provided for both backends
- [ ] Configuration options documented

---

## Phase 4: Production Readiness

**Goal**: Telemetry, monitoring, and production deployment.

**Estimated Effort**: 2-3 days

### Tasks

#### 4.1: Add Telemetry

**Purpose**: Track which backend handles which queries.

**Implementation** (`runtime/telemetry.rs`):
```rust
pub struct QueryMetrics {
    pub backend: BackendType,
    pub duration_ms: u64,
    pub rows_returned: usize,
    pub memory_bytes: usize,
}

pub enum BackendType {
    FraiseWire,
    ArrowPolars,
    PostgresSQL,
}

impl Executor {
    async fn execute(&self, query: &str, variables: Option<&Value>) -> Result<String> {
        let start = Instant::now();
        let backend_type = self.classify_backend(query)?;

        let result = match backend_type {
            BackendType::FraiseWire => self.execute_with_wire(...).await?,
            BackendType::ArrowPolars => self.execute_with_arrow(...).await?,
            BackendType::PostgresSQL => self.execute_with_postgres(...).await?,
        };

        // Record metrics
        self.record_query_metrics(QueryMetrics {
            backend: backend_type,
            duration_ms: start.elapsed().as_millis() as u64,
            rows_returned: result.len(),
            memory_bytes: result.memory_usage(),
        });

        Ok(result)
    }
}
```

**Acceptance Criteria**:
- [ ] Backend selection tracked
- [ ] Latency measured per backend
- [ ] Memory usage recorded
- [ ] Metrics exported (Prometheus format)

---

#### 4.2: Add Health Checks

**File**: `crates/fraiseql-server/src/health.rs`

**Endpoints**:
```rust
GET /health
{
  "status": "healthy",
  "backends": {
    "wire": { "status": "healthy", "pool": { "idle": 12, "active": 4 } },
    "arrow": { "status": "healthy", "cache_hit_rate": 0.85 }
  }
}

GET /ready
{
  "ready": true,
  "backends": ["wire", "arrow"]
}
```

**Acceptance Criteria**:
- [ ] Health endpoints respond correctly
- [ ] Backend status checked individually
- [ ] Pool metrics exposed
- [ ] Ready check verifies all backends

---

#### 4.3: Configuration Management

**File**: `config.toml` (example)

```toml
[database]
# Connection strings
standard_connection = "postgres://localhost/mydb"
analytics_connection = "postgres://localhost/mydb"

[backends]
# Backend selection
use_wire_backend = true
use_arrow_backend = true

[wire_backend]
pool_size = 16
chunk_size = 256
enable_tls = true

[arrow_backend]
cache_enabled = true
cache_size_mb = 1024
parallel_execution = true
```

**Acceptance Criteria**:
- [ ] Configuration file parsed correctly
- [ ] Backend selection configurable
- [ ] Pool sizes configurable
- [ ] Defaults sensible for production

---

## Timeline & Milestones

### Week 1: Foundation
- **Days 1-2**: Phase 0 (Fix build errors, establish baseline)
- **Days 3-5**: Phase 1.1-1.3 (Dependencies, WHERE generator, connection pool)

**Milestone**: fraiseql-wire adapter compiles and passes unit tests

---

### Week 2-3: Wire Backend
- **Days 6-8**: Phase 1.4 (FraiseWireAdapter implementation)
- **Days 9-10**: Phase 1.5-1.6 (Module exports, benchmarks)
- **Days 11-12**: Phase 1 testing and iteration

**Milestone**: FraiseWireAdapter passes all integration tests

---

### Week 3-4: Arrow Backend
- **Days 13-15**: Phase 2.1-2.2 (Arrow dependencies, adapter implementation)
- **Days 16-18**: Phase 2.3 (Executor integration)
- **Days 19-20**: Phase 2 testing

**Milestone**: Dual backend system routes queries correctly

---

### Week 5: Testing & Production
- **Days 21-23**: Phase 3 (Integration tests, performance validation)
- **Days 24-26**: Phase 4 (Telemetry, health checks, configuration)
- **Days 27-28**: Documentation and final validation

**Milestone**: Production-ready dual backend system

---

## Success Criteria

### Functional
- [ ] All existing tests pass without modification
- [ ] Standard queries route to fraiseql-wire backend
- [ ] Analytics queries route to Arrow/Polars backend
- [ ] WHERE clause translation supports all operators
- [ ] Connection pooling works efficiently
- [ ] Health checks verify backend availability

### Performance
- [ ] No latency regression for standard queries (<5% variance)
- [ ] Memory usage reduced by 1000x+ for queries >10K rows
- [ ] Throughput maintained (>100K rows/sec)
- [ ] Connection pools scale to 32+ concurrent queries

### Quality
- [ ] Zero clippy warnings (pedantic mode)
- [ ] 100% of integration tests passing
- [ ] Comprehensive documentation
- [ ] Performance benchmarks documented
- [ ] Configuration examples provided

---

## Rollback Plan

If integration reveals critical issues:

### Phase 1 Rollback (Wire Backend)
```rust
// Use feature flag to disable wire backend
cargo build --no-default-features --features postgres

// Executor falls back to PostgresAdapter
```

### Phase 2 Rollback (Arrow Backend)
```rust
// Keep SQL-based analytics execution
// Disable Arrow backend via feature flag
cargo build --features wire-backend  // without analytics
```

### Full Rollback
```rust
// Use original PostgresAdapter only
cargo build --no-default-features --features postgres
```

**Safety**: Feature flags ensure smooth rollback without code changes.

---

## Dependencies & Risks

### External Dependencies
- **fraiseql-wire**: v0.1.0 (local path dependency)
  - Risk: API changes, need to vendor or coordinate releases
  - Mitigation: Lock to specific git commit, vendor if needed

- **Arrow/Polars**: Rapidly evolving ecosystem
  - Risk: Breaking changes in new releases
  - Mitigation: Pin versions, thorough testing before upgrades

### Internal Dependencies
- **Build errors must be fixed first** (Phase 0)
  - Blocking: Cannot proceed without clean build
  - Priority: CRITICAL

- **WHERE clause coverage**
  - Risk: Some operators may not translate correctly
  - Mitigation: Comprehensive test suite, gradual rollout

### Resource Dependencies
- **Test database**: Requires Postgres 17 with test data
- **Benchmarking**: Requires stable environment for accurate measurements
- **Memory profiling**: Requires heaptrack or similar tools

---

## Communication Plan

### Weekly Updates
- Status report every Friday
- Blockers identified and escalated
- Performance metrics tracked

### Milestones
- Phase 0 complete: Build clean, baseline established
- Phase 1 complete: Wire adapter functional
- Phase 2 complete: Dual backend working
- Phase 3-4 complete: Production ready

### Success Metrics
- Memory reduction: Target 10,000x+ for 100K row queries
- Latency: Within 5% of baseline
- Test coverage: 100% of integration tests passing
- Documentation: Architecture, performance, configuration

---

## Next Actions

1. **Immediate (Today)**:
   - [ ] Fix build errors (Phase 0.1)
   - [ ] Create baseline benchmark suite (Phase 0.2)

2. **This Week**:
   - [ ] Implement WHERE SQL generator (Phase 1.2)
   - [ ] Implement connection pool (Phase 1.3)
   - [ ] Start FraiseWireAdapter (Phase 1.4)

3. **Next Week**:
   - [ ] Complete FraiseWireAdapter
   - [ ] Integration tests
   - [ ] Performance comparison

---

**Plan Status**: READY FOR IMPLEMENTATION
**Total Estimated Effort**: 4-6 weeks
**Risk Level**: Medium (manageable with incremental approach)
**Next Phase**: Phase 0 - Prerequisites & Foundation
