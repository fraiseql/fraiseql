# Quick Reference - Phase 3.2 ProductionPool Implementation

## Current State

**Commit**: `0cdae0c6`
**Status**: Phase 3.2 Foundation Complete, ProductionPool Implementation Ready

## Key Architectural Pattern

```rust
// FraiseQL's JSONB Pattern (CORRECT)
let results = pool.query("SELECT data FROM tv_user LIMIT 10").await?;
// Returns: Vec<serde_json::Value>
// Each element is JSONB from column 0

// QueryParam for Parameters (ALWAYS USE THIS)
let results = pool.query_with_params(
    "SELECT data FROM tv_user WHERE id = $1",
    vec![QueryParam::BigInt(123)]
).await?;
```

## Type-Safe Parameters

```rust
// Use QueryParam for ALL user input
pub enum QueryParam {
    Null,                           // SQL NULL
    Bool(bool),                     // BOOLEAN
    Int(i32),                       // INTEGER
    BigInt(i64),                    // BIGINT
    Float(f32),                     // REAL
    Double(f64),                    // DOUBLE PRECISION
    Text(String),                   // TEXT/VARCHAR
    Json(serde_json::Value),        // JSONB
    Timestamp(chrono::NaiveDateTime), // TIMESTAMP
    Uuid(uuid::Uuid),               // UUID
}

// Before execution, validate:
validate_parameter_count(sql, &params)?;
prepare_parameters(&params)?;
```

## PoolBackend Trait

```rust
#[async_trait]
pub trait PoolBackend: Send + Sync {
    /// Execute SELECT query, extract JSONB from column 0
    async fn query(&self, sql: &str) -> PoolResult<Vec<serde_json::Value>>;

    /// Execute INSERT/UPDATE/DELETE, return affected rows
    async fn execute(&self, sql: &str) -> PoolResult<u64>;

    /// Pool metadata/info
    fn pool_info(&self) -> serde_json::Value;
    fn backend_name(&self) -> &str;
}
```

## Error Handling

```rust
pub enum PoolError {
    ConnectionAcquisition(String),  // Failed to get connection
    QueryExecution(String),         // Query failed or parameter invalid
    Configuration(String),          // Pool configuration issue
}
```

## Parameter Binding Module

**Location**: `fraiseql_rs/src/db/parameter_binding.rs`

```rust
// Main functions
pub fn prepare_parameters(params: &[QueryParam]) -> PoolResult<()>
pub fn count_placeholders(sql: &str) -> usize
pub fn validate_parameter_count(sql: &str, params: &[QueryParam]) -> PoolResult<()>
pub fn format_parameter(param: &QueryParam) -> String

// Always validate before execution:
validate_parameter_count(sql, &params)?;
prepare_parameters(&params)?;
```

## ProductionPool Structure

**Location**: `fraiseql_rs/src/db/pool_production.rs`

```rust
#[derive(Debug, Clone)]
pub struct ProductionPool {
    pool: Arc<Pool>,                    // deadpool::Pool<Manager>
    config: DatabaseConfig,             // Connection config
    metrics: Arc<PoolMetrics>,          // Metrics collection
}

impl ProductionPool {
    pub fn new(config: DatabaseConfig) -> DatabaseResult<Self> { ... }

    // TODO: Add these methods
    // pub async fn query(&self, sql: &str) -> PoolResult<Vec<serde_json::Value>>
    // pub async fn execute(&self, sql: &str) -> PoolResult<u64>
    // For transactions: begin_transaction, commit_transaction, rollback_transaction
}
```

## View Naming Convention

**Always use singular, never plural:**

- ✅ `tv_user` - Projection/materialized table
- ✅ `v_user` - Virtual view
- ❌ `tv_users` - WRONG (plural)
- ❌ `users_view` - WRONG (wrong naming pattern)

## Common Commands

```bash
# Build (check for compilation errors)
cargo build --lib

# Build with Clippy (strict linting)
cargo build --lib --all-targets

# Format code
cargo fix --lib -p fraiseql

# Test (7467+ tests)
python -m pytest tests/ -q

# Commit with specific message
git commit -m "feat(phase-3.2): Your message here"

# Check git status
git status
```

## Files to Edit Tomorrow

### Priority Order:

1. **fraiseql_rs/src/db/pool_production.rs**
   - Implement `query()` method
   - Use deadpool connection to execute SELECT
   - Extract JSONB from column 0
   - Return as Vec<serde_json::Value>

2. **fraiseql_rs/src/db/pool.rs** (if needed)
   - Add Python bindings for new methods
   - Update examples with tv_user naming

3. **Tests** (after implementation)
   - Create integration tests
   - Test parameter binding
   - Test error handling

## Implementation Checklist for Task 4

- [ ] Read deadpool-postgres documentation for Query API
- [ ] Implement `query()` method in ProductionPool
- [ ] Use `validate_parameter_count()` for validation
- [ ] Extract JSONB from column 0 of results
- [ ] Handle errors with PoolError::QueryExecution
- [ ] Add execution time measurement
- [ ] Write unit tests
- [ ] Write integration tests with real PostgreSQL
- [ ] Verify parameter binding works
- [ ] Check that test suite still passes

## Common Pitfalls to Avoid

❌ **Don't**: Transform rows to JSON in Rust
✅ **Do**: Let PostgreSQL handle JSON in column 0

❌ **Don't**: Create new PoolError variants
✅ **Do**: Use existing error types (QueryExecution, Configuration, etc.)

❌ **Don't**: Pass raw strings as parameters
✅ **Do**: Use QueryParam enum exclusively

❌ **Don't**: Skip parameter validation
✅ **Do**: Always call validate_parameter_count() and prepare_parameters()

❌ **Don't**: Use plural names for views
✅ **Do**: Use singular (tv_user, v_user)

## Terminal Setup for Tomorrow

```bash
# Navigate to project
cd /home/lionel/code/fraiseql

# Check status
git log --oneline -3
git status

# Build to verify no errors
cargo build --lib

# Run tests to establish baseline
python -m pytest tests/ -q --tb=short

# Start implementing Task 4
# Edit: fraiseql_rs/src/db/pool_production.rs
```

## Documentation References

**In Repository:**
- `PHASE_3_2_ARCHITECTURE_REVIEW.md` - Full architectural patterns
- `PHASE_3_2_FOUNDATION_COMPLETE.md` - Implementation details
- `fraiseql_rs/src/db/pool/README.md` - Pool abstraction overview
- `fraiseql_rs/src/db/parameter_binding.rs` - Code and inline docs

**External:**
- deadpool-postgres: Connection pool for Tokio
- tokio-postgres: Async PostgreSQL driver
- PostgreSQL JSONB documentation

## Expected Outcomes for Tomorrow

**By End of Day:**
- ✅ Task 4: `query()` method fully implemented and tested
- ✅ All integration tests passing
- ✅ No new compilation errors
- ✅ Commit: `feat(phase-3.2): Implement query execution in ProductionPool`

**Not Today (but soon):**
- Task 5: Transactions (tomorrow afternoon?)
- Task 6: Mutations (day after?)

---

**Good luck tomorrow! Everything is ready. The foundation is solid. 🚀**
