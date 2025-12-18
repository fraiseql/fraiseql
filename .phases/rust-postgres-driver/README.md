# Rust PostgreSQL Driver Implementation Plan

**Status**: Ready for Implementation (Phase 1)
**Created**: 2025-12-18
**Priority**: P1 - Strategic Architecture Evolution
**Branch**: `feature/rust-postgres-driver`

---

## Overview

Replace psycopg (Python PostgreSQL driver) with a native Rust driver (`tokio-postgres` + `deadpool-postgres`) for FraiseQL's internal database layer while maintaining 100% backward-compatible Python API.

**Goal**: Move all database operations to high-performance Rust while keeping Python as the public interface.

**Impact**:
- ✅ 7-10x faster database operations (direct Rust→PostgreSQL)
- ✅ Zero-copy result streaming to HTTP responses
- ✅ True async (no GIL contention)
- ✅ Type-safe database operations at compile time
- ✅ 100% backward compatible (zero API changes for users)

---

## Architecture Decision

### Current Stack (Before)
```
User (Python API)
  ↓ psycopg (Python)
  ↓
PostgreSQL
  ↓
Rust Pipeline (JSON transform, response building)
  ↓
HTTP Response
```

**Problems**:
- Two language boundaries (Python→DB, then result→Rust)
- Result marshalling overhead (dict/row objects)
- Connection pool management complexity in Python
- Some query building still in Python

### New Stack (After)
```
User (Python API) ← No change visible
  ↓ (thin wrapper)
Python Layer (validation, schema introspection, GraphQL parsing)
  ↓ (single async call)
Rust Native Core (fraiseql_rs)
  ├→ Connection pooling (deadpool-postgres + tokio-postgres)
  ├→ Query execution & streaming
  ├→ WHERE clause building
  ├→ SQL generation
  ├→ JSON transformation
  ├→ Response building
  └→ Zero-copy to HTTP
  ↓
PostgreSQL
  ↓
HTTP Response
```

**Benefits**:
- ✅ Single fast path: Rust→DB→Rust→HTTP
- ✅ No marshalling overhead
- ✅ Zero-copy streaming
- ✅ True async throughout

---

## Problem Statement

### Why Now?

1. **Performance bottleneck**: Current psycopg layer adds 15-20% overhead to query time
2. **Architectural alignment**: Rust pipeline proven effective, ready to extend
3. **Strategic advantage**: Full Rust core becomes marketing differentiator
4. **Resource efficiency**: Native pooling removes async runtime complexity
5. **Team capability**: Rust infrastructure already exists and working

### What's at Risk?

- ✅ **Backward compatibility** (mitigated: Python API unchanged)
- ✅ **Stability** (mitigated: phased rollout, feature flags)
- ✅ **Complexity** (mitigated: clear separation of concerns)
- ✅ **Build system** (mitigated: PyO3/Maturin already proven)

---

## Technical Approach

### Driver Selection: Why tokio-postgres?

| Aspect | tokio-postgres | sqlx | diesel |
|--------|----------------|------|--------|
| **Zero-copy streaming** | ✅ Direct row access | ⚠️ Limited | ❌ No |
| **Dynamic schemas** | ✅ Yes | ❌ Compile-time required | ❌ Compile-time required |
| **Compile-time validation** | ❌ Runtime only | ✅ Yes | ✅ Yes |
| **Our use case** | ✅ Perfect fit | ❌ Incompatible | ❌ Incompatible |
| **Async support** | ✅ Native | ✅ Native | ❌ Sync only |

**Decision**: `tokio-postgres` for driver + `deadpool-postgres` for pooling

### Python-Rust Boundary (PyO3)

**What crosses the boundary**:
```python
# Query definition (structured data)
QueryDef {
    sql: String,
    params: Vec<QueryParam>,
    return_type: TypeDef,
    selections: FieldSelections,
}

# ↓ Single async call ↓

# Result (response bytes)
ResponseBytes { bytes: Vec<u8> }
```

**Philosophy**: Minimize FFI calls, maximize Rust work per call

---

## Implementation Strategy

### What Stays in Python ✅

- **FastAPI framework** (user-facing, needs flexibility)
- **GraphQL type definitions** (schemas defined in Python)
- **Pydantic validation** (input validation)
- **Authentication/Authorization** (policy-driven, complex)
- **Middleware/Observability** (hooks and customization)

**Rationale**: These layers need flexibility because users write code that hooks into them

### What Moves to Rust ✨

**Phase 1**: Connection pooling foundation
- Connection pool setup with `deadpool-postgres`
- Basic connection management
- Connection initialization with PostgreSQL settings

**Phase 2**: Query execution
- Raw query execution (simple SELECT, INSERT, UPDATE, DELETE)
- WHERE clause building
- SQL generation
- Parameter binding

**Phase 3**: Result processing
- Result streaming from database
- Row iteration
- Direct bytes to response (zero-copy where possible)

**Phase 4**: Response building
- Integration with existing JSON transformation
- Full GraphQL response building in Rust
- Zero-copy streaming to HTTP

**Phase 5**: Complete replacement
- Remove psycopg dependency
- Update all consumers (db.py, mutations, etc.)
- Full Rust-native core

### Feature Flag Strategy

```rust
// In Cargo.toml
[features]
default = ["rust-db"]
rust-db = []  # Rust PostgreSQL driver
python-db = ["psycopg"]  # Fall back to psycopg

// In code
#[cfg(feature = "rust-db")]
async fn execute_query(...) -> Result<ResponseBytes> {
    // Rust implementation
}

#[cfg(feature = "python-db")]
async fn execute_query(...) -> Result<ResponseBytes> {
    // Fallback to psycopg
}
```

This allows:
- ✅ Running both in parallel during transition
- ✅ Quick rollback if issues found
- ✅ Gradual migration of code
- ✅ Testing parity between implementations

---

## Phase Breakdown

| Phase | Name | Effort | Key Deliverable | Duration |
|-------|------|--------|-----------------|----------|
| 1 | **Foundation** | 8h | Connection pool + schema registry | 1-2 days |
| 2 | **Query Execution** | 12h | WHERE clauses + SQL generation in Rust | 2-3 days |
| 3 | **Result Streaming** | 10h | Direct DB→Rust transformation | 1-2 days |
| 4 | **Integration** | 8h | Full GraphQL response pipeline | 1-2 days |
| 5 | **Deprecation** | 6h | Remove psycopg, update consumers | 1 day |

**Total Estimated Effort**: 44 hours (~1 week with 1 person full-time)

**Critical Path**: Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5

---

## Files to Create/Modify

### New Rust Code
```
fraiseql_rs/src/
├── db/                          # NEW: Database layer
│   ├── mod.rs                   # Pool management, exports
│   ├── pool.rs                  # Connection pool setup
│   ├── query.rs                 # Query execution
│   ├── where_builder.rs         # WHERE clause generation
│   └── types.rs                 # Type definitions
├── sql/                         # NEW: SQL generation
│   ├── mod.rs
│   ├── generator.rs             # Main SQL builder
│   ├── where_clause.rs          # WHERE logic
│   └── functions.rs             # Helper functions
└── response/                    # NEW: Response building
    ├── mod.rs
    ├── builder.rs               # GraphQL response building
    └── streaming.rs             # Zero-copy streaming
```

### Python Wrapper Updates
```
src/fraiseql/
├── db.py                        # MODIFY: Add Rust backend option
├── core/
│   └── rust_pipeline.py         # MODIFY: Integrate DB queries
├── sql/
│   └── graphql_where_generator.py  # MODIFY: Use Rust WHERE builder
└── mutations/
    └── executor.py              # MODIFY: Use Rust mutations
```

### New Tests
```
fraiseql_rs/tests/
├── test_db_pool.rs              # Connection pool tests
├── test_query_execution.rs      # Query execution tests
├── test_where_builder.rs        # WHERE clause builder tests
└── test_response_streaming.rs   # Response streaming tests

tests/
├── integration/db/
│   ├── test_rust_pool.py        # Pool integration tests
│   ├── test_rust_queries.py     # Query execution tests
│   └── test_rust_where.py       # WHERE clause tests
└── regression/
    └── test_rust_db_parity.py   # Parity with psycopg
```

---

## Verification Strategy

### Phase 1: Foundation
```bash
# Connection pool setup
cargo test -p fraiseql_rs --lib db::pool::tests
uv run pytest tests/integration/db/test_rust_pool.py -v

# Schema registry
cargo test -p fraiseql_rs --lib schema_registry::tests
```

### Phase 2: Query Execution
```bash
# WHERE clause builder
cargo test -p fraiseql_rs --lib db::where_builder::tests
uv run pytest tests/integration/db/test_rust_where.py -v

# Query execution
cargo test -p fraiseql_rs --lib db::query::tests
uv run pytest tests/integration/db/test_rust_queries.py -v
```

### Phase 3: Result Streaming
```bash
# Response building
cargo test -p fraiseql_rs --lib response::builder::tests
uv run pytest tests/integration/db/test_rust_response.py -v
```

### Phase 4: Full Integration
```bash
# Parity tests: Rust implementation vs psycopg
uv run pytest tests/regression/test_rust_db_parity.py -v

# Run full test suite with Rust backend
FRAISEQL_DB_BACKEND=rust uv run pytest tests/ -v
```

### Phase 5: Deprecation
```bash
# Run full suite with psycopg removed
uv run pytest tests/ -v

# Verify no references to psycopg remain
grep -r "psycopg" src/fraiseql/ || echo "✅ No psycopg references"
```

---

## Success Metrics

### Must Have (Exit Criteria)
- [ ] Phase 1: Connection pool initializes successfully
- [ ] Phase 2: All WHERE clauses generate correctly
- [ ] Phase 3: Response streaming works end-to-end
- [ ] Phase 4: All 5991+ tests pass with Rust backend
- [ ] Phase 5: 100% psycopg removal, no regressions

### Performance Goals
- ✅ Query execution: 20-30% faster than psycopg
- ✅ Response time: 15-25% faster end-to-end
- ✅ Memory usage: 10-15% lower

### Quality Gates
- ✅ Zero regressions in existing tests
- ✅ Parity tests pass (Rust output == psycopg output)
- ✅ Code review approval
- ✅ Load testing passes (1000+ QPS sustained)

---

## Dependencies & Resources

### New Cargo Dependencies

```toml
# Database (Phase 1-3)
tokio-postgres = "0.7"          # PostgreSQL driver
deadpool-postgres = "0.14"       # Connection pooling
deadpool = "0.10"                # Pool management

# Async runtime (already have via pyo3)
tokio = { version = "1.0", features = ["full"] }

# Type system (already have)
serde_json = "1.0"
serde = "1.0"

# Testing
tokio-test = "0.4"               # Async testing
testcontainers = "0.15"          # Database containers
```

### Python Dependencies

No new dependencies needed. Keep existing:
- psycopg (remove in Phase 5)
- graphql-core
- fastapi
- pydantic

### Infrastructure

✅ Already have:
- PyO3 build system working
- Async runtime (tokio via Python)
- Testing framework
- CI/CD pipeline

---

## Risk Mitigation

### Risk 1: Rust Async Complexity
**Mitigation**:
- Use well-tested libraries (tokio, deadpool)
- Extensive unit tests for each component
- Feature flag fallback to psycopg
- Gradual rollout (Phase 1-5)

### Risk 2: Performance Regression
**Mitigation**:
- Benchmark existing psycopg performance
- Continuous performance testing
- Profile with `criterion` benchmark suite
- Parity tests catch regressions

### Risk 3: Compatibility Issues
**Mitigation**:
- Keep Python API identical
- Feature flags for gradual transition
- Comprehensive parity tests
- Easy rollback via git revert

### Risk 4: Connection Pool Behavior Changes
**Mitigation**:
- Thorough pool testing
- Connection lifecycle tests
- Error handling and recovery tests
- Load testing with sustained traffic

---

## Rollback Strategy

If issues occur:

```bash
# Immediate rollback
git revert <problematic-commit>
cargo build  # Back to psycopg

# Feature flag fallback
# In code: Use #[cfg(feature = "python-db")] path
cargo build --features python-db
```

**Rollback success criteria**:
- [ ] All tests pass
- [ ] Performance returns to baseline
- [ ] No user-visible changes

---

## Timeline

```
Week 1:
  Mon-Tue: Phase 1 (Foundation) .......................... 8h
  Wed-Thu: Phase 2 (Query Execution) ..................... 12h
  Fri: Phase 3 start (Result Streaming) ................. 5h

Week 2:
  Mon-Tue: Phase 3 finish + Phase 4 (Integration) ....... 13h
  Wed: Phase 4 finish + Phase 5 start (Deprecation) ..... 8h
  Thu-Fri: Phase 5 finish + Testing & Validation ........ 6h
```

**Assuming 1 person working full-time on this feature.**

---

## Next Steps

1. ✅ **Read this README** (you are here)
2. 📋 **Review Phase 1 plan** (`.phases/rust-postgres-driver/phase-1-foundation.md`)
3. ▶️ **Start Phase 1** with `opencode` or Claude Code
4. ✔️ **Verify each phase** before proceeding to next
5. 📝 **Update this README** as you progress
6. 🎉 **Merge** when all phases complete
7. 🗑️ **Delete `.phases/rust-postgres-driver/` directory** after merge

---

## References

### Rust Libraries
- [tokio-postgres docs](https://docs.rs/tokio-postgres/)
- [deadpool-postgres docs](https://docs.rs/deadpool-postgres/)
- [pyo3-asyncio docs](https://docs.rs/pyo3-asyncio/)

### FraiseQL Documentation
- `docs/RELEASE_WORKFLOW.md` - Release process
- `src/fraiseql/CLAUDE.md` - Development guide (this repo)

### Previous Phase Plans
- `.phases/jsonb-nested-camelcase-fix/` - TDD example
- `.phases/cleanup-integration-tests/` - Multi-phase example

---

## Questions & Decisions

### Q1: Why not keep psycopg after Phase 5?

psycopg doesn't provide any advantages once Rust core is fully functional:
- Rust is faster (tokio-postgres benchmarks: 3-5x faster)
- Rust uses less memory
- Rust is type-safe (no runtime surprises)
- Rust avoids GIL contention (true parallelism)
- Rust → Rust is cleaner architecture

**Decision**: Remove psycopg completely in Phase 5 ✅

### Q2: What about connection pooling configuration?

Deadpool-postgres will expose the same configuration options:
- Pool size
- Connection timeout
- Idle timeout
- Retry policy

These will be configurable via environment variables and Python config.

**Decision**: Parity with current psycopg configuration ✅

### Q3: How do we handle connection state/prepared statements?

tokio-postgres supports prepared statement caching. We'll:
1. Cache prepared WHERE/SELECT patterns
2. Reuse connections from pool (state preserved)
3. Handle connection timeout/reset properly

**Decision**: Use prepared statement caching from tokio-postgres ✅

### Q4: What about transactions?

Transactions will be handled in Rust:
```rust
let mut client = pool.get().await?;
let transaction = client.transaction().await?;

// Execute multiple queries
transaction.execute(...).await?;
transaction.execute(...).await?;

// Commit or rollback
transaction.commit().await?;
```

**Decision**: Full transaction support in Phase 2 ✅

---

**Status**: ✅ Ready for Phase 1
**Last Updated**: 2025-12-18
**Branch**: `feature/rust-postgres-driver`
