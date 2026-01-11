# FraiseQL v2: Rust Core Implementation Guide

**Date:** 2026-01-11
**Status:** Ready for Phase 2 Implementation

---

## Quick Start

You've completed the architecture design for FraiseQL v2's Rust core. Here's how to proceed with implementation:

### 📚 **Read These Documents In Order:**

1. **[RUST_CORE_ARCHITECTURE.md](./RUST_CORE_ARCHITECTURE.md)** - Complete architecture design (50+ pages)
   - Module structure
   - Trait definitions
   - Type designs
   - WHERE generation algorithm
   - JSONB projection algorithm
   - Authorization strategy
   - Trade-off analysis
   - Migration plan

2. **[CODE_EXAMPLES.md](./CODE_EXAMPLES.md)** - Concrete code examples (30+ pages)
   - End-to-end query execution
   - Database adapter implementation
   - WHERE clause generation examples
   - JSONB projection examples
   - Field-level authorization examples
   - Caching integration

3. **This file** - Implementation checklist and workflow

---

## Architecture Summary

### The FraiseQL v2 Execution Model

**Key Insight:** FraiseQL does NOT generate complex SQL. Instead:

```
Compile-time: Create views that return denormalized JSONB
    ↓
Runtime: Execute simple SELECT data FROM v_X WHERE ...
    ↓
Rust: Project JSONB to requested fields + apply auth masking
```

**This means:**
- ✅ No complex JOIN generation
- ✅ No field list generation
- ✅ Just WHERE clause + JSONB projection
- ✅ Database does aggregation, Rust does filtering

### Module Structure

```
fraiseql-core/src/
├── db/                  🔧 Phase 2 (6 days)
│   ├── traits.rs        - DatabaseAdapter trait
│   ├── pool.rs          - Connection pooling (deadpool)
│   ├── where_builder.rs - WHERE clause AST
│   ├── where_gen.rs     - WHERE SQL generation
│   └── postgres/        - PostgreSQL implementation
│
├── runtime/             🔧 Phase 5 (12-15 days)
│   ├── executor.rs      - Query execution pipeline
│   ├── projector.rs     - JSONB → GraphQL projection
│   ├── selection.rs     - SelectionSet types
│   └── auth_mask.rs     - Field-level auth masking
│
├── cache/               🔧 Phase 2 (2 days)
│   ├── backend.rs       - CacheBackend trait
│   ├── memory.rs        - In-memory LRU cache
│   └── key_gen.rs       - Cache key generation
│
└── security/            🔧 Phase 3 (2 days)
    ├── auth_context.rs  - User roles, permissions
    └── field_auth.rs    - Field-level auth rules
```

### Core Traits

```rust
// 1. Database operations
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>>;
}

// 2. WHERE clause generation
pub trait WhereClauseGenerator {
    fn generate(
        &self,
        where_clause: &WhereClause,
        bindings: &TypeBindings,
    ) -> Result<(String, Vec<QueryParameter>)>;
}

// 3. JSONB projection
pub trait JsonbProjector {
    fn project(
        &self,
        jsonb: &serde_json::Value,
        selection_set: &SelectionSet,
        auth_mask: &AuthMask,
    ) -> Result<serde_json::Value>;
}

// 4. Caching
#[async_trait]
pub trait CacheBackend: Send + Sync {
    async fn get(&self, key: &CacheKey) -> Result<Option<CachedValue>>;
    async fn set(&self, key: &CacheKey, value: &CachedValue, ttl: Option<Duration>) -> Result<()>;
}
```

### Key Design Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| **JSONB Parsing** | `serde_json::Value` | Battle-tested, start simple, optimize later |
| **WHERE Builder** | AST (`WhereClause` enum) | Type-safe, composable, SQL injection proof |
| **Connection Pool** | `deadpool` | Production-ready, good metrics |
| **Auth Mask** | `HashMap<String, FieldAuthRule>` | Simple, flexible, O(1) lookups |
| **Projection** | Clone-based (now), Cow-based (future) | Correctness first, optimize hot paths later |

---

## Phase 2: Database Layer Implementation (6 days)

### Day 1-2: Database Abstraction

**Files to create:**

```
crates/fraiseql-core/src/db/
├── mod.rs                      # Module exports + docs
├── traits.rs                   # DatabaseAdapter trait
├── pool.rs                     # Connection pool helpers
├── types.rs                    # DatabaseType, JsonbValue, PoolMetrics
└── postgres/
    ├── mod.rs
    └── adapter.rs              # PostgresAdapter implementation
```

**Dependencies to add to `Cargo.toml`:**

```toml
[dependencies]
tokio-postgres = { version = "0.7", features = ["with-serde_json-1"] }
deadpool-postgres = "0.13"
async-trait = "0.1"
```

**Implementation checklist:**

- [ ] Define `DatabaseAdapter` trait in `traits.rs`
- [ ] Define supporting types in `types.rs`
- [ ] Create `PostgresAdapter` struct
- [ ] Implement connection pooling with deadpool
- [ ] Implement `execute_where_query()` method
- [ ] Implement `health_check()` method
- [ ] Implement `pool_metrics()` method
- [ ] Write unit tests for adapter
- [ ] Write integration test with real PostgreSQL

**Test command:**

```bash
cargo test --package fraiseql-core --lib db
```

---

### Day 3-4: WHERE Clause Generation

**Files to create:**

```
crates/fraiseql-core/src/db/
├── where_builder.rs            # WhereClause AST types
├── where_gen.rs                # WhereClauseGenerator trait
└── postgres/
    └── where_gen.rs            # PostgreSQL WHERE generator
```

**Implementation checklist:**

- [ ] Define `WhereClause` enum (Field, And, Or, Not)
- [ ] Define `WhereOperator` enum (all v1 operators)
- [ ] Implement `WhereOperator::from_str()`
- [ ] Define `QueryParameter` enum
- [ ] Define `WhereClauseGenerator` trait
- [ ] Implement `PostgresWhereGenerator`
- [ ] Handle simple field conditions (eq, neq, gt, etc.)
- [ ] Handle string operators (icontains, like, etc.)
- [ ] Handle nested JSONB paths (EXISTS with jsonb_array_elements)
- [ ] Handle logical operators (AND, OR, NOT)
- [ ] Write comprehensive unit tests (50+ operators)
- [ ] Write integration tests (complex nested WHERE)

**Test cases to cover:**

```rust
#[test] fn test_eq_operator()
#[test] fn test_icontains_operator()
#[test] fn test_nested_array_filter()
#[test] fn test_and_or_composition()
#[test] fn test_not_operator()
#[test] fn test_all_operators() // Golden file test
```

---

### Day 5-6: Integration & MySQL/SQLite

**Files to create:**

```
crates/fraiseql-core/src/db/
├── mysql/
│   ├── mod.rs
│   ├── adapter.rs
│   └── where_gen.rs
└── sqlite/
    ├── mod.rs
    ├── adapter.rs
    └── where_gen.rs
```

**Implementation checklist:**

- [ ] Implement `MySQLAdapter` (similar to PostgreSQL)
- [ ] Implement `MySQLWhereGenerator`
  - Use `JSON_EXTRACT()` instead of `->`
  - Use `JSON_UNQUOTE()` for string extraction
- [ ] Implement `SQLiteAdapter`
- [ ] Implement `SQLiteWhereGenerator`
  - Use `json_extract()` function
  - Handle case-sensitive LIKE
- [ ] Write cross-database integration tests
- [ ] Benchmark WHERE generation (< 1ms target)

**Acceptance criteria for Phase 2:**

- ✅ Execute `SELECT data FROM v_user WHERE ...` queries
- ✅ All v1 WHERE operators supported (50+ operators)
- ✅ PostgreSQL, MySQL, SQLite adapters working
- ✅ 90%+ test coverage for WHERE generation
- ✅ Performance: < 1ms WHERE generation overhead
- ✅ Integration tests pass on all databases

---

## Phase 3: Security Layer (2 days)

### Day 1: Field-Level Authorization

**Files to create:**

```
crates/fraiseql-core/src/security/
├── mod.rs                      # Module exports
├── auth_context.rs             # UserContext type
├── field_auth.rs               # FieldAuthRule, AuthMask
└── query_auth.rs               # Query-level auth (future)
```

**Implementation checklist:**

- [ ] Define `UserContext` struct (user_id, roles, permissions, tenant_id)
- [ ] Define `FieldAuthRule` struct (required_roles, required_permissions)
- [ ] Define `AuthMask` struct with rule lookup
- [ ] Implement `AuthMask::from_schema()`
- [ ] Implement `AuthMask::is_field_authorized()`
- [ ] Write unit tests for auth rules
- [ ] Write integration tests (admin vs viewer scenarios)

---

### Day 2: Integration with Projector

**Files to update:**

```
crates/fraiseql-core/src/runtime/
└── projector.rs                # Add auth checks during projection
```

**Implementation checklist:**

- [ ] Update `JsonbProjector::project()` to check auth mask
- [ ] Silently omit unauthorized fields
- [ ] Test auth masking with complex scenarios
- [ ] Document auth behavior

**Acceptance criteria for Phase 3:**

- ✅ Unauthorized fields silently omitted from responses
- ✅ Role + permission support working
- ✅ Auth tests cover admin, viewer, support scenarios
- ✅ Auth checks integrated into JSONB projection

---

## Phase 5: Runtime Executor + JSONB Projection (12-15 days)

### Day 1-3: JSONB Projection

**Files to create:**

```
crates/fraiseql-core/src/runtime/
├── mod.rs                      # Module exports
├── selection.rs                # SelectionSet types
└── projector.rs                # DefaultJsonbProjector
```

**Implementation checklist:**

- [ ] Define `SelectionSet` struct
- [ ] Define `FieldSelection` struct
- [ ] Define `FieldSelectionType` enum (Leaf, Object, Array)
- [ ] Implement helper methods (has_field, get_nested_selection)
- [ ] Define `JsonbProjector` trait
- [ ] Implement `DefaultJsonbProjector`
- [ ] Handle scalar fields (Leaf)
- [ ] Handle object fields (nested projection)
- [ ] Handle array fields (map over elements)
- [ ] Support field aliasing
- [ ] Write projection unit tests (30+ test cases)

**Test cases:**

```rust
#[test] fn test_simple_projection()
#[test] fn test_nested_object_projection()
#[test] fn test_array_projection()
#[test] fn test_field_alias()
#[test] fn test_deep_nesting()
```

---

### Day 4-6: Runtime Executor

**Files to create:**

```
crates/fraiseql-core/src/runtime/
├── executor.rs                 # Executor struct
└── context.rs                  # ExecutionContext
```

**Implementation checklist:**

- [ ] Define `Executor` struct
- [ ] Define `ExecutionContext` (user, tenant, request_id)
- [ ] Implement query execution pipeline:
  1. Parse GraphQL query → SelectionSet + WhereClause
  2. Execute database query (via DatabaseAdapter)
  3. Project JSONB (via JsonbProjector)
  4. Apply auth mask
  5. Return GraphQL response
- [ ] Add error handling at each stage
- [ ] Add logging/tracing (use `tracing` crate)
- [ ] Write executor integration tests

**Dependencies:**

```toml
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

### Day 7-9: Caching Integration

**Files to create:**

```
crates/fraiseql-core/src/cache/
├── mod.rs                      # Module exports
├── backend.rs                  # CacheBackend trait
├── memory.rs                   # MemoryCache (LRU)
├── key_gen.rs                  # Cache key generation
└── invalidation.rs             # Cache invalidation helpers
```

**Dependencies:**

```toml
lru = "0.12"
sha2 = "0.10"
```

**Implementation checklist:**

- [ ] Define `CacheBackend` trait
- [ ] Define `CacheKey`, `CachedValue`, `CacheStats`
- [ ] Implement `MemoryCache` with LRU eviction
- [ ] Implement cache key generation (SHA-256 hash)
- [ ] Implement pattern-based invalidation
- [ ] Integrate cache into executor pipeline
- [ ] Write cache tests (hit, miss, invalidation)

---

### Day 10-12: Optimization & Benchmarks

**Files to create:**

```
benches/
├── where_generation.rs         # Benchmark WHERE generation
├── projection.rs               # Benchmark JSONB projection
└── executor.rs                 # Benchmark end-to-end execution
```

**Dependencies:**

```toml
[dev-dependencies]
criterion = "0.5"
```

**Benchmarking checklist:**

- [ ] Benchmark WHERE generation (target: < 1ms)
- [ ] Benchmark JSONB projection (target: < 5ms for 100 fields)
- [ ] Benchmark end-to-end query execution (target: < 10ms)
- [ ] Profile hot paths with `cargo flamegraph`
- [ ] Optimize allocation-heavy code
- [ ] Consider zero-copy projection (Cow)

**Commands:**

```bash
cargo bench
cargo flamegraph --bench executor
```

**Acceptance criteria for Phase 5:**

- ✅ End-to-end query execution working
- ✅ JSONB projection accurate (all test cases pass)
- ✅ Field-level auth enforced (unauthorized fields omitted)
- ✅ Caching working (95%+ hit rate on repeated queries)
- ✅ Performance targets met:
  - p50 < 2ms (simple queries)
  - p99 < 10ms (complex queries)
  - 10,000+ queries/sec (single instance)

---

## Development Workflow

### Daily Development Cycle

```bash
# 1. Pull latest changes
git pull origin feature/phase-2-database

# 2. Create feature branch
git checkout -b db/postgres-adapter

# 3. Implement feature
# ... write code in crates/fraiseql-core/src/db/postgres/adapter.rs

# 4. Run tests
cargo test --package fraiseql-core --lib db::postgres

# 5. Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# 6. Format code
cargo fmt

# 7. Commit
git add .
git commit -m "feat(db): Implement PostgreSQL adapter

## Changes
- Add PostgresAdapter struct
- Implement DatabaseAdapter trait
- Add connection pooling with deadpool
- Add health check functionality

## Verification
✅ cargo test passes
✅ cargo clippy clean
"

# 8. Push
git push -u origin db/postgres-adapter

# 9. Create PR (when ready)
gh pr create --title "feat(db): PostgreSQL adapter implementation"
```

### Test-Driven Development (TDD)

**Always write tests first:**

```rust
// 1. Write failing test
#[test]
fn test_icontains_where() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Icontains,
        value: json!("example.com"),
    };

    let gen = PostgresWhereGenerator;
    let (sql, params) = gen.generate(&clause, &TypeBindings::default()).unwrap();

    assert_eq!(sql, "data->>'email' ILIKE $1");
    assert_eq!(params[0], QueryParameter::String("%example.com%"));
}

// 2. Run test (should fail)
// cargo test test_icontains_where

// 3. Implement feature
// ... write code to make test pass

// 4. Run test (should pass)
// cargo test test_icontains_where

// 5. Refactor if needed

// 6. Run all tests
// cargo test
```

---

## Quality Checklist

Before merging any PR, verify:

- [ ] **Tests pass**: `cargo test --all-targets --all-features`
- [ ] **Clippy clean**: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] **Formatted**: `cargo fmt -- --check`
- [ ] **Documented**: All public APIs have doc comments
- [ ] **Coverage**: New code has 90%+ test coverage
- [ ] **Performance**: Benchmarks show no regressions
- [ ] **Integration**: End-to-end tests pass
- [ ] **Security**: No SQL injection, auth bypasses, or unsafe code

**CI will enforce all of these.**

---

## Troubleshooting

### Common Issues

**1. Clippy errors:**

```bash
# Auto-fix where possible
cargo clippy --fix --allow-dirty

# Check specific package
cargo clippy --package fraiseql-core
```

**2. Test failures:**

```bash
# Run single test with output
cargo test test_name -- --nocapture

# Run tests in specific module
cargo test db::postgres

# Run tests with logging
RUST_LOG=debug cargo test
```

**3. Database connection errors:**

```bash
# Check PostgreSQL is running
pg_isready

# Check connection string
echo $DATABASE_URL

# Test connection manually
psql $DATABASE_URL
```

**4. Performance issues:**

```bash
# Profile with flamegraph
cargo flamegraph --bench executor

# Profile with perf
cargo build --release
perf record --call-graph dwarf ./target/release/fraiseql-server
perf report
```

---

## Resources

### Documentation

- **Architecture**: `docs/architecture/RUST_CORE_ARCHITECTURE.md`
- **Examples**: `docs/architecture/CODE_EXAMPLES.md`
- **API Docs**: `cargo doc --open`

### External Resources

- [tokio-postgres docs](https://docs.rs/tokio-postgres)
- [deadpool docs](https://docs.rs/deadpool)
- [async-trait docs](https://docs.rs/async-trait)
- [serde_json docs](https://docs.rs/serde_json)

### Community

- GitHub Issues: Report bugs, ask questions
- GitHub Discussions: Design discussions, RFCs

---

## Next Steps

1. **Review architecture documents** (this + RUST_CORE_ARCHITECTURE.md + CODE_EXAMPLES.md)
2. **Get approval** from team/stakeholders
3. **Create Phase 2 branch**: `git checkout -b feature/phase-2-database`
4. **Start Day 1 tasks**: Implement `DatabaseAdapter` trait
5. **Follow TDD workflow**: Write tests first, then implementation
6. **Daily standups**: Share progress, blockers, learnings

---

## Success Criteria Summary

### Phase 2 (Database Layer) - 6 days

- ✅ PostgreSQL adapter working
- ✅ MySQL adapter working (basic)
- ✅ SQLite adapter working (basic)
- ✅ All WHERE operators supported (50+)
- ✅ Nested JSONB path filtering working
- ✅ 90%+ test coverage
- ✅ Performance: < 1ms WHERE generation

### Phase 3 (Security) - 2 days

- ✅ Field-level authorization working
- ✅ Role + permission support
- ✅ Auth mask integrated into projection
- ✅ Auth tests covering all scenarios

### Phase 5 (Runtime + Projection) - 12-15 days

- ✅ JSONB projection accurate
- ✅ Nested object/array projection working
- ✅ Field aliasing supported
- ✅ End-to-end query execution working
- ✅ Caching integrated (95%+ hit rate)
- ✅ Performance targets met:
  - p50 < 2ms
  - p99 < 10ms
  - 10,000+ queries/sec

---

**Ready to build the best GraphQL engine the world has ever seen!** 🚀
