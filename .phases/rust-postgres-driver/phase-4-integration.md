# Phase 4: Full Integration - GraphQL Response Pipeline

**Phase**: 4 of 5
**Effort**: 8 hours
**Status**: Blocked until Phase 3 complete
**Prerequisite**: Phase 3 - Result Streaming complete

---

## Objective

Complete the GraphQL response pipeline with mutations and full integration:
1. Integrate result streaming with JSON transformation
2. Implement mutations in Rust
3. Full end-to-end GraphQL query execution
4. Comprehensive testing and validation

**Success Criteria**:
- ✅ All GraphQL queries execute end-to-end in Rust
- ✅ Mutations work correctly
- ✅ All 5991+ tests pass with Rust backend
- ✅ Performance: 20-30% faster than psycopg throughout

---

## What Gets Integrated

### Query Execution
```
GraphQL Query
    ↓ Parse (Python)
    ↓ Validate (Python)
    ↓ Extract QueryDef
    ↓ Single FFI call → Rust
Rust:
  ├→ Build WHERE clause
  ├→ Build SELECT SQL
  ├→ Execute query (streaming)
  ├→ Transform results (streaming)
  ├→ Build GraphQL response
    ↓
Response bytes
```

### Mutation Execution
```
GraphQL Mutation
    ↓ Parse (Python)
    ↓ Validate (Python)
    ↓ Extract MutationDef
    ↓ Single FFI call → Rust
Rust:
  ├→ Build INSERT/UPDATE/DELETE SQL
  ├→ Execute within transaction
  ├→ Execute post-mutation query
  ├→ Transform results
  ├→ Build GraphQL response
    ↓
Response bytes
```

---

## Implementation Overview

### Key Changes

**Python Layer** (`src/fraiseql/core/rust_pipeline.py`):
- Consolidate all database operations into single Rust call
- Remove psycopg direct calls (use Rust layer)
- Keep GraphQL parsing and validation in Python

**Rust Layer** (`fraiseql_rs/src/`):
- Complete query execution pipeline
- Mutation handling
- Transaction support
- Error handling and mapping

### Feature Flags

Enable full Rust backend option:
```toml
[features]
default = ["rust-db"]
rust-db = []  # Full Rust database layer
```

---

## Testing Strategy

### Integration Tests
```bash
# End-to-end query tests
uv run pytest tests/integration/graphql/test_rust_queries.py -v

# End-to-end mutation tests
uv run pytest tests/integration/graphql/test_rust_mutations.py -v

# Complex query tests
uv run pytest tests/integration/graphql/test_rust_complex.py -v
```

### Parity Tests
```bash
# Verify Rust results == psycopg results
uv run pytest tests/regression/test_rust_db_parity.py -v

# Run 5991+ test suite with Rust backend
FRAISEQL_DB_BACKEND=rust uv run pytest tests/ -v --tb=short
```

### Performance Tests
```bash
# Sustained load testing
uv run pytest tests/performance/test_rust_throughput.py -v

# Memory profiling
cargo bench --bench memory
```

---

## Verification Commands

### Build
```bash
cargo build --release -p fraiseql_rs
uv run pip install -e .
```

### Quick Check
```bash
# Query tests
uv run pytest tests/integration/graphql/test_rust_queries.py::TestQueries::test_simple_select -v

# Mutation tests
uv run pytest tests/integration/graphql/test_rust_mutations.py::TestMutations::test_insert -v
```

### Full Validation
```bash
# All integration tests
uv run pytest tests/integration/ -v

# All regression tests
uv run pytest tests/regression/ -v

# Full test suite with Rust backend
FRAISEQL_DB_BACKEND=rust uv run pytest tests/ -v
```

---

## Success Metrics

- [ ] All 5991+ tests pass with Rust backend
- [ ] Query performance: 20-30% faster than psycopg
- [ ] Mutation performance: 15-25% faster than psycopg
- [ ] Memory usage: 10-15% lower
- [ ] Zero regressions
- [ ] Code coverage: ≥ 85%

---

## Known Challenges

### Complex Queries
- Multi-table joins (handle in this phase)
- Nested field selections (already supported by Rust pipeline)
- Aggregations (if supported, add support)

### Edge Cases
- NULL handling in JSONB
- Large result sets (> 100K rows)
- Concurrent requests
- Transaction rollbacks

### Performance
- Query plan optimization
- Index usage verification
- Connection pool efficiency under load

---

## Rollback Plan

If critical issues found:

```bash
# Immediate rollback
FRAISEQL_DB_BACKEND=psycopg uv run pytest tests/ -v

# Or via git
git revert <commit>
```

---

## Next Phase

After Phase 4 is complete and verified:

👉 Proceed to **Phase 5: Deprecation** to remove psycopg dependency

---

**Status**: ✅ Ready for Phase 3 completion
**Duration**: 8 hours
**Branch**: `feature/rust-postgres-driver`
