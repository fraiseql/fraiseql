# FraiseQL-Wire Integration Assessment

**Date**: 2026-01-13
**Context**: Evaluating fraiseql-wire as a query backend for FraiseQL v2
**Status**: Analysis Phase

---

## Executive Summary

**fraiseql-wire** is a purpose-built, streaming JSON query engine that could serve as a specialized backend for FraiseQL v2's **standard (non-analytics) queries**. It offers **20,000x memory savings** and **streaming-first semantics** that align well with FraiseQL's compiled query architecture.

### Key Recommendation

**Dual Data Plane Architecture**:

- ✅ **fraiseql-wire**: Standard GraphQL queries (entity lookups, simple WHERE)
- ✅ **Arrow + Polars**: Analytics queries (aggregations, window functions, fact tables)
- ✅ **Tokio**: Async runtime unifying both data planes

This architecture leverages the strengths of each component:

- **fraiseql-wire** for memory-efficient streaming of entity data
- **Arrow/Polars** for high-performance analytical transformations
- **Unified API** that routes queries transparently based on query type

---

## fraiseql-wire: Technical Profile

### What It Is

A **minimal, async Rust query engine** that:

- Implements Postgres Simple Query Protocol from scratch (no libpq)
- Streams JSON data with bounded memory: **O(chunk_size)** vs **O(result_size)**
- Provides 1000x-20,000x memory savings for large result sets
- Uses Tokio for async I/O

### Supported Query Shape

```sql
SELECT data
FROM v_{entity}
WHERE <predicate>
[ORDER BY expression]
```

**Constraints**:

- Exactly **one column** (JSON/JSONB)
- Read-only (no writes, transactions, or prepared statements)
- Single active query per connection

### Performance Characteristics (v0.1.0 Benchmarks)

| Metric | fraiseql-wire | tokio-postgres | Advantage |
|--------|---------------|----------------|-----------|
| **Memory (10K rows)** | 1.3 KB | 2.6 MB | **2,000x** |
| **Memory (100K rows)** | 1.3 KB | 26 MB | **20,000x** |
| **Memory (1M rows)** | 1.3 KB | 260 MB | **200,000x** |
| **Time-to-first-row** | 2-5 ms | 2-5 ms | **Same** |
| **Throughput** | 100K-500K rows/sec | 100K-500K rows/sec | **Same** |
| **Connection overhead** | ~250 ns | ~250 ns | **Same** |

**Key Insight**: Memory advantage scales linearly with result size. No latency penalty.

### Security Audit Status

- ✅ **Zero unsafe code** (full memory safety)
- ✅ **Zero known vulnerabilities** (157 crates audited)
- ✅ **SCRAM-SHA-256 authentication**
- ✅ **TLS support** (rustls-based)
- ⚠ **TLS required for production TCP** (Phase 8 roadmap)

---

## FraiseQL v2: Current Architecture

### Query Execution Flow

```
GraphQL Query → Executor → Classify Query Type
                              ↓
               ┌──────────────┼──────────────┐
               ↓              ↓              ↓
          Regular       Aggregate        Window
               ↓              ↓              ↓
       DatabaseAdapter  AggregationSQL  WindowSQL
               ↓              ↓              ↓
          Postgres      Postgres        Postgres
         (standard)   (fact tables)  (window fns)
```

### DatabaseAdapter Trait

```rust
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>>;

    fn database_type(&self) -> DatabaseType;
    async fn health_check(&self) -> Result<()>;
    fn pool_metrics(&self) -> PoolMetrics;
}
```

**Current Implementation**: `PostgresAdapter` (uses tokio-postgres or sqlx)

### Query Type Classification

FraiseQL v2 already has a **query type router** in `executor.rs:12-25`:

```rust
enum QueryType {
    Regular,              // Standard entity queries
    Aggregate(String),    // Analytics queries (_aggregate)
    Window(String),       // Window function queries (_window)
}
```

**Perfect match** for dual data plane architecture!

---

## Integration Strategy: Dual Data Plane

### Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│                    FraiseQL Executor                          │
│                  (runtime/executor.rs)                        │
└────────────────┬────────────────────────────────┬─────────────┘
                 │                                │
        ┌────────┴─────────┐           ┌────────┴─────────────┐
        │ Regular Queries  │           │ Analytics Queries    │
        │  (QueryType::    │           │  (QueryType::        │
        │   Regular)       │           │   Aggregate/Window)  │
        └────────┬─────────┘           └────────┬─────────────┘
                 │                                │
                 ↓                                ↓
    ┌────────────────────────┐       ┌─────────────────────────┐
    │  fraiseql-wire         │       │  Arrow Data Plane       │
    │  (Streaming Backend)   │       │  (Analytics Backend)    │
    ├────────────────────────┤       ├─────────────────────────┤
    │ • Stream<JsonValue>    │       │ • RecordBatch pipeline  │
    │ • O(chunk_size) memory │       │ • Polars aggregations   │
    │ • WHERE clause         │       │ • Window functions      │
    │ • ORDER BY             │       │ • Fact table queries    │
    │ • Tokio async          │       │ • Calendar dimensions   │
    └────────────────────────┘       └─────────────────────────┘
                 │                                │
                 └────────────┬───────────────────┘
                              ↓
                    ┌──────────────────────┐
                    │  Postgres 17         │
                    │  • Views (v_*)       │
                    │  • Fact tables       │
                    │  • JSONB data        │
                    └──────────────────────┘
```

### Implementation Approach

#### Phase 1: Add fraiseql-wire as Alternative Backend (Non-Analytics)

**Goal**: Create `FraiseWireAdapter` implementing `DatabaseAdapter` trait.

**Files to Modify**:

```
crates/fraiseql-core/
├── Cargo.toml                              # Add fraiseql-wire dependency
├── src/db/
│   ├── mod.rs                              # Export FraiseWireAdapter
│   └── fraiseql_wire_adapter.rs            # NEW: Adapter implementation
└── src/runtime/
    └── executor.rs                         # Optional: add adapter selection
```

**Adapter Implementation** (`fraiseql_wire_adapter.rs`):

```rust
use async_trait::async_trait;
use fraiseql_wire::{FraiseClient, Error as WireError};
use futures::StreamExt;

pub struct FraiseWireAdapter {
    client: FraiseClient,
}

impl FraiseWireAdapter {
    pub async fn new(connection_string: &str) -> Result<Self> {
        let client = FraiseClient::connect(connection_string).await
            .map_err(|e| FraiseQLError::Connection(format!("fraiseql-wire: {}", e)))?;
        Ok(Self { client })
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
        // Build query using fraiseql-wire's QueryBuilder
        let mut query = self.client.query(view);

        // Translate WhereClause to SQL predicate
        if let Some(clause) = where_clause {
            let sql = self.where_clause_to_sql(clause)?;
            query = query.where_sql(&sql);
        }

        // Add ORDER BY if needed (from WhereClause metadata)
        // query = query.order_by("data->>'created_at' DESC");

        // Execute and stream results
        let mut stream = query.execute().await?;

        // Collect results (with limit/offset)
        let mut results = Vec::new();
        let mut count = 0;
        let skip = offset.unwrap_or(0);
        let take = limit.unwrap_or(u32::MAX);

        while let Some(item) = stream.next().await {
            let json = item?;

            // Apply offset
            if count < skip {
                count += 1;
                continue;
            }

            // Apply limit
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
        // Execute simple query to verify connectivity
        let mut stream = self.client
            .query("pg_catalog.pg_type")
            .where_sql("oid = 16") // boolean type
            .execute()
            .await?;

        stream.next().await.ok_or_else(|| {
            FraiseQLError::Connection("Health check failed".into())
        })??;

        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        // fraiseql-wire doesn't expose connection pool metrics
        // Return default metrics
        PoolMetrics::default()
    }
}
```

**Benefits**:

- ✅ Drop-in replacement for `PostgresAdapter`
- ✅ Same trait interface (`DatabaseAdapter`)
- ✅ Streaming semantics reduce memory pressure
- ✅ No changes to executor or query planning logic

**Challenges**:

- ⚠ **WHERE clause translation**: Need to convert `WhereClause` AST → SQL string
- ⚠ **Pagination**: `LIMIT`/`OFFSET` must be applied in Rust (post-stream)
- ⚠ **Connection pooling**: fraiseql-wire has 1 connection per client

#### Phase 2: Arrow Data Plane for Analytics

**Goal**: Route analytics queries to Arrow/Polars pipeline.

**Architecture**:

```rust
// In executor.rs
match query_type {
    QueryType::Regular => {
        // Use fraiseql-wire adapter
        self.execute_regular_query_with_wire(query, variables).await
    }
    QueryType::Aggregate(query_name) => {
        // Use Arrow + Polars
        self.execute_aggregate_with_arrow(&query_name, variables).await
    }
    QueryType::Window(query_name) => {
        // Use Arrow + Polars
        self.execute_window_with_arrow(&query_name, variables).await
    }
}
```

**Analytics Backend** (`db/arrow_adapter.rs`):

```rust
pub struct ArrowAnalyticsAdapter {
    // Arrow-based query execution
    polars_ctx: PolarsContext,
    postgres_pool: PgPool, // For initial data loading
}

impl ArrowAnalyticsAdapter {
    pub async fn execute_aggregate(
        &self,
        fact_table: &str,
        dimensions: &[String],
        measures: &[MeasureDef],
        filters: Option<&WhereClause>,
    ) -> Result<RecordBatch> {
        // 1. Load data from Postgres fact table
        let df = self.load_fact_table(fact_table, filters).await?;

        // 2. Apply aggregations using Polars
        let result = df
            .groupby(dimensions)?
            .agg(&measures.iter().map(|m| m.to_polars_expr()).collect())?;

        // 3. Convert to Arrow RecordBatch
        Ok(result.to_arrow()?)
    }
}
```

**Benefits**:

- ✅ Polars optimized for aggregations (SIMD, parallel execution)
- ✅ Arrow columnar format perfect for analytics
- ✅ Native support for window functions
- ✅ Can cache intermediate results efficiently

#### Phase 3: Unified Query Router

**Goal**: Transparent query routing based on query type.

**Executor Configuration**:

```rust
pub struct Executor {
    schema: CompiledSchema,

    // Dual data plane
    standard_adapter: Arc<FraiseWireAdapter>,
    analytics_adapter: Arc<ArrowAnalyticsAdapter>,

    // Query routing
    matcher: QueryMatcher,
    planner: QueryPlanner,
    config: RuntimeConfig,
}

impl Executor {
    pub fn new_dual_plane(
        schema: CompiledSchema,
        standard_conn: &str,
        analytics_conn: &str,
    ) -> Result<Self> {
        let standard_adapter = Arc::new(
            FraiseWireAdapter::new(standard_conn).await?
        );
        let analytics_adapter = Arc::new(
            ArrowAnalyticsAdapter::new(analytics_conn).await?
        );

        Ok(Self {
            schema,
            standard_adapter,
            analytics_adapter,
            matcher: QueryMatcher::new(schema.clone()),
            planner: QueryPlanner::new(true),
            config: RuntimeConfig::default(),
        })
    }
}
```

**Benefits**:

- ✅ Single API surface for all query types
- ✅ Optimal backend selection per query
- ✅ Easy to add telemetry (which backend handled which query)

---

## Tokio Integration

### Current Status

Both FraiseQL v2 and fraiseql-wire **already use Tokio**:

- **FraiseQL v2**: `tokio = { version = "1", features = ["full"] }`
- **fraiseql-wire**: `tokio = { version = "1", features = ["full"] }`

### Compatibility

✅ **Perfect compatibility** - both use:

- Tokio 1.x async runtime
- `async_trait` for trait methods
- `futures` for `Stream` trait

### No Integration Work Required

The async runtimes are **already unified**. No additional integration needed.

---

## WHERE Clause Translation Challenge

### Problem

FraiseQL v2 uses a **WHERE clause AST** (`WhereClause` enum):

```rust
pub enum WhereClause {
    Field {
        path: Vec<String>,
        operator: WhereOperator,
        value: serde_json::Value,
    },
    And(Vec<WhereClause>),
    Or(Vec<WhereClause>),
    Not(Box<WhereClause>),
}
```

fraiseql-wire expects a **SQL string**:

```rust
query.where_sql("data->>'status' = 'active'")
```

### Solution: WHERE Clause Generator

**Create a SQL generator** for fraiseql-wire adapter:

```rust
impl FraiseWireAdapter {
    fn where_clause_to_sql(&self, clause: &WhereClause) -> Result<String> {
        match clause {
            WhereClause::Field { path, operator, value } => {
                let json_path = self.build_json_path(path);
                let sql_op = self.operator_to_sql(operator);
                let sql_value = self.value_to_sql(value);

                Ok(format!("{} {} {}", json_path, sql_op, sql_value))
            }
            WhereClause::And(clauses) => {
                let parts: Vec<_> = clauses
                    .iter()
                    .map(|c| self.where_clause_to_sql(c))
                    .collect::<Result<_>>()?;
                Ok(format!("({})", parts.join(" AND ")))
            }
            WhereClause::Or(clauses) => {
                let parts: Vec<_> = clauses
                    .iter()
                    .map(|c| self.where_clause_to_sql(c))
                    .collect::<Result<_>>()?;
                Ok(format!("({})", parts.join(" OR ")))
            }
            WhereClause::Not(clause) => {
                let inner = self.where_clause_to_sql(clause)?;
                Ok(format!("NOT ({})", inner))
            }
        }
    }

    fn build_json_path(&self, path: &[String]) -> String {
        if path.len() == 1 {
            format!("data->>'{}', path[0])
        } else {
            let nested_path = path[..path.len()-1]
                .iter()
                .map(|s| format!("'{}'", s))
                .collect::<Vec<_>>()
                .join(", ");
            format!("data#>'{{{}}}'->>'{}', nested_path, path.last().unwrap())
        }
    }

    fn operator_to_sql(&self, op: &WhereOperator) -> &'static str {
        match op {
            WhereOperator::Eq => "=",
            WhereOperator::Ne => "!=",
            WhereOperator::Gt => ">",
            WhereOperator::Gte => ">=",
            WhereOperator::Lt => "<",
            WhereOperator::Lte => "<=",
            WhereOperator::Contains => "LIKE", // Use % wildcards in value
            WhereOperator::Icontains => "ILIKE",
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Endswith => "LIKE",
            WhereOperator::In => "= ANY",
        }
    }
}
```

**Reuse Existing Code**: FraiseQL v2 already has `WhereClauseGenerator` trait with PostgreSQL implementation. Can extract and reuse this logic.

---

## Connection Pooling Consideration

### fraiseql-wire Design

fraiseql-wire uses **1 connection per client**:

- Each `FraiseClient` owns a single `Connection`
- Streams are single-query (no multiplexing)

### Integration Options

**Option 1: Client Pool** (Recommended)

```rust
pub struct FraiseWireAdapter {
    connection_string: String,
    client_pool: Pool<FraiseClient>,
    max_connections: usize,
}

impl FraiseWireAdapter {
    pub async fn new(connection_string: &str, pool_size: usize) -> Result<Self> {
        let pool = Pool::builder()
            .max_size(pool_size)
            .build(|| FraiseClient::connect(connection_string))
            .await?;

        Ok(Self {
            connection_string: connection_string.to_string(),
            client_pool: pool,
            max_connections: pool_size,
        })
    }

    async fn execute_where_query(&self, ...) -> Result<Vec<JsonbValue>> {
        // Acquire client from pool
        let client = self.client_pool.get().await?;

        // Execute query
        let mut stream = client.query(view).execute().await?;

        // ... process stream ...

        // Client automatically returned to pool on drop
        Ok(results)
    }
}
```

**Option 2: Connection Pool Inside fraiseql-wire** (Future Enhancement)

Modify fraiseql-wire to support connection pooling internally:

```rust
// In fraiseql-wire (future enhancement)
pub struct FraiseClient {
    pool: Arc<ConnectionPool>,
}

impl FraiseClient {
    pub async fn connect_pooled(conn_str: &str, pool_size: usize) -> Result<Self> {
        // ...
    }
}
```

**Recommendation**: Start with Option 1 (client pool in adapter). If performance is insufficient, contribute pooling back to fraiseql-wire.

---

## Performance Implications

### Memory Usage Improvement

**Before** (tokio-postgres):

- Query returning 100K rows: **26 MB** memory
- Query returning 1M rows: **260 MB** memory

**After** (fraiseql-wire):

- Query returning 100K rows: **1.3 KB** memory (configurable chunk size)
- Query returning 1M rows: **1.3 KB** memory

**Improvement**: **20,000x** memory reduction for large result sets.

### Latency Impact

**Time-to-first-row**: No change (~2-5 ms for both)
**Throughput**: No change (100K-500K rows/sec)

**Key Insight**: Memory savings come with **zero latency penalty**.

### When It Matters Most

**High-impact scenarios**:

- ✅ Large list queries (e.g., `users(limit: 10000)`)
- ✅ Cursor-based pagination (streaming > buffering)
- ✅ Real-time subscriptions (future feature)
- ✅ Export queries (CSV, JSON streaming)

**Low-impact scenarios**:

- ⚠ Small queries (<1000 rows) - no significant difference
- ⚠ Analytics queries - Arrow/Polars more important

---

## Migration Path

### Phase 0: Prerequisites

- [ ] Fix build errors in FraiseQL v2 tests (missing `fact_tables`, `calendar_dimensions` fields)
- [ ] Verify tokio-postgres adapter works end-to-end
- [ ] Benchmark current memory usage for baseline

### Phase 1: fraiseql-wire Integration (Standard Queries)

**Estimated Effort**: 2-3 days

- [ ] Add `fraiseql-wire` dependency to `fraiseql-core/Cargo.toml`
- [ ] Implement `FraiseWireAdapter` in `db/fraiseql_wire_adapter.rs`
- [ ] Extract WHERE clause generator (reuse from `db/postgres.rs`)
- [ ] Add client pooling (using `deadpool` or `bb8`)
- [ ] Write integration tests comparing tokio-postgres vs fraiseql-wire
- [ ] Add feature flag: `cargo build --features wire-backend`

**Acceptance Criteria**:

- All existing tests pass with `FraiseWireAdapter`
- Memory usage reduced for queries >10K rows
- No latency regression

### Phase 2: Arrow Analytics Backend

**Estimated Effort**: 5-7 days

- [ ] Add `arrow`, `polars`, `datafusion` dependencies
- [ ] Implement `ArrowAnalyticsAdapter` in `db/arrow_adapter.rs`
- [ ] Port aggregation SQL generation to Polars expressions
- [ ] Port window function SQL to Arrow compute kernels
- [ ] Add fact table → DataFrame loader
- [ ] Write integration tests for analytics queries

**Acceptance Criteria**:

- All `_aggregate` queries route to Arrow backend
- All `_window` queries route to Arrow backend
- Performance matches or exceeds SQL-based approach

### Phase 3: Unified Executor

**Estimated Effort**: 1-2 days

- [ ] Modify `Executor` to support dual adapters
- [ ] Add query type routing logic
- [ ] Add telemetry (track which backend handled which query)
- [ ] Update documentation and examples

**Acceptance Criteria**:

- Single `Executor::execute()` API routes transparently
- Metrics show correct backend selection
- All existing tests pass without modification

---

## Risk Assessment

### Low Risk ✅

- **Tokio compatibility**: Both already use Tokio 1.x
- **Query shape match**: fraiseql-wire's `SELECT data FROM v_*` matches FraiseQL exactly
- **Trait compatibility**: `DatabaseAdapter` trait is a perfect abstraction boundary
- **Security**: fraiseql-wire has passed comprehensive security audit

### Medium Risk ⚠

- **WHERE clause translation**: Need to ensure all operators are correctly mapped
  - *Mitigation*: Extensive integration tests, reuse existing generator code
- **Connection pooling**: fraiseql-wire doesn't have built-in pooling
  - *Mitigation*: Implement client pool in adapter (standard pattern)
- **Pagination**: `LIMIT`/`OFFSET` must be applied in Rust
  - *Mitigation*: Early termination in stream iteration (minimal overhead)

### High Risk (Manageable) 🔴

- **Arrow/Polars integration complexity**: Analytics backend requires deep integration
  - *Mitigation*: Incremental migration, keep SQL backend as fallback
- **Two data planes to maintain**: More complexity than single backend
  - *Mitigation*: Clear separation of concerns, comprehensive test coverage

---

## Alternative Approaches Considered

### Alternative 1: Single Backend (tokio-postgres only)

**Pros**:

- ✅ Simple (no dual backend complexity)
- ✅ Mature, battle-tested driver

**Cons**:

- ❌ High memory usage for large result sets (26 MB for 100K rows)
- ❌ No streaming semantics (buffers entire result)
- ❌ Not optimized for analytics (row-oriented)

**Verdict**: Not recommended for FraiseQL's use case.

### Alternative 2: Single Backend (Arrow/Polars only)

**Pros**:

- ✅ Excellent analytics performance
- ✅ Columnar format efficient for aggregations

**Cons**:

- ❌ Overkill for simple entity lookups
- ❌ Higher overhead for small queries
- ❌ More complex integration

**Verdict**: Good for analytics, but not ideal for standard queries.

### Alternative 3: Dual Backend (fraiseql-wire + Arrow/Polars)

**Pros**:

- ✅ Best of both worlds (memory efficiency + analytics performance)
- ✅ Query router already exists in FraiseQL v2
- ✅ Clean separation of concerns
- ✅ Tokio compatibility is seamless

**Cons**:

- ⚠ Two backends to maintain
- ⚠ WHERE clause translation needed

**Verdict**: **Recommended approach** - leverages strengths of each component.

---

## Recommendations

### Short-Term (Next 1-2 Weeks)

1. **Fix Critical Build Errors**: Resolve missing `fact_tables` and `calendar_dimensions` fields
2. **Benchmark Baseline**: Measure current memory usage for 10K, 100K, 1M row queries
3. **Prototype Integration**: Implement `FraiseWireAdapter` with WHERE clause translation
4. **Integration Tests**: Compare tokio-postgres vs fraiseql-wire memory usage

### Medium-Term (Next 1-2 Months)

1. **Production Integration**: Make fraiseql-wire the default backend for standard queries
2. **Arrow Analytics Backend**: Implement dual data plane with query routing
3. **Performance Monitoring**: Add telemetry to track backend usage and performance
4. **Documentation**: Update architecture docs with dual backend design

### Long-Term (Next 6 Months)

1. **Streaming Subscriptions**: Leverage fraiseql-wire's streaming for real-time updates
2. **Incremental Loading**: Stream large result sets to client with backpressure
3. **Edge Functions**: Deploy FraiseQL with minimal memory footprint (fraiseql-wire advantage)

---

## Open Questions

1. **Connection Pool Size**: What's optimal pool size for fraiseql-wire clients?
   - *Recommendation*: Start with 16-32, benchmark under load

2. **WHERE Clause Coverage**: Does fraiseql-wire support all FraiseQL operators?
   - *Answer*: Yes, all operators map to standard SQL predicates

3. **TLS Requirements**: Is TLS mandatory for production?
   - *Answer*: Yes for TCP, fraiseql-wire supports rustls-based TLS

4. **Arrow Integration Complexity**: Is Polars the right choice for analytics?
   - *Recommendation*: Evaluate Polars vs DataFusion, benchmark both

5. **Fallback Strategy**: What happens if fraiseql-wire query fails?
   - *Recommendation*: Add feature flag to fallback to tokio-postgres

---

## Conclusion

**fraiseql-wire is an excellent fit for FraiseQL v2's standard query execution.**

### Key Strengths

- ✅ **Memory efficiency**: 20,000x reduction for large result sets
- ✅ **Query shape alignment**: `SELECT data FROM v_*` matches FraiseQL exactly
- ✅ **Tokio compatibility**: Seamless async runtime integration
- ✅ **Security**: Comprehensive audit, zero unsafe code
- ✅ **Trait compatibility**: Drop-in replacement for `DatabaseAdapter`

### Recommended Architecture

**Dual Data Plane**:

- **fraiseql-wire**: Standard queries (entity lookups, simple filtering)
- **Arrow + Polars**: Analytics queries (aggregations, window functions, fact tables)
- **Unified Router**: `Executor` routes queries based on type classification

### Next Steps

1. Fix build errors (immediate)
2. Prototype `FraiseWireAdapter` (1-2 days)
3. Integration tests (1 day)
4. Production integration (1 week)
5. Arrow analytics backend (2-3 weeks)

**Estimated total effort**: 4-6 weeks for full dual backend implementation.

---

**Status**: Ready for implementation. See `.claude/plans/fraiseql-wire-integration-plan.md` for detailed phase breakdown.
