# FraiseQL Rust SQL Layer Roadmap

## At a Glance

```
CURRENT STATE (v1.9.5)          TARGET STATE (v3.0+)
────────────────────────────────────────────────────────

Users: Python Code              Users: Python Code (SAME!)
       ↓                               ↓
Framework: Python + Rust        Framework: Python Wrapper
       ├─ gql/builders                ├─ gql/builders
       ├─ db/repository              ├─ db/repository
       └─ sql/ (3500 LOC)    →       └─ fraiseql_rs/ (Rust FFI)
             ↓                             ↓
Database: PostgreSQL            Database: PostgreSQL

KEY: Users see NO CHANGES. Performance improves dramatically.
```

---

## Timeline & Phases

### Phase A: Foundation (✅ COMPLETE - Jan 2026)
**Status**: Done
- ✅ Rust schema export (11 tests)
- ✅ Python schema loader (10 tests)
- ✅ WHERE generator integration (8 tests)
- ✅ OrderBy generator integration (15 tests)
- ✅ Performance testing (7 tests)
- ✅ 68 tests total, 383 regressions prevented

**Key Achievement**: Schema is now in Rust. Python accesses via caching layer.

**Time to complete**: 2 weeks
**Team**: 1 person
**Result**: 64.87ns cached access, 2.3-4.4x speedup

---

### Phase B: Query Building (Q2-Q3 2026)
**Timeline**: 6-9 months
**Team**: 2-3 engineers
**Status**: Planned

#### B.1: WHERE Clause Generation (Month 1-3)
```rust
fraiseql_rs/src/query_builder.rs
├─ build_where_clause(filters) → SQL string
├─ Handle all operators (string, numeric, etc.)
├─ Support nested conditions (AND, OR, NOT)
└─ Generate type-safe SQL

Replaces: fraiseql/sql/where_generator.py (480 LOC)
Replaces: fraiseql/sql/where/*.py (40 files)
```

**Expected Performance**: 5-10x faster WHERE clause generation

#### B.2: ORDER BY Clause Generation (Month 2-4)
```rust
fraiseql_rs/src/query_builder.rs
├─ build_order_by_clause(ordering) → SQL string
├─ Handle nested orderings
├─ Support ASC/DESC
└─ Merge with WHERE clause if needed

Replaces: fraiseql/sql/order_by_generator.py (240 LOC)
```

**Expected Performance**: 2-5x faster ORDER BY generation

#### B.3: Complete Query Building (Month 3-5)
```rust
fraiseql_rs/src/query_builder.rs
├─ SELECT query building
├─ UPDATE query building
├─ DELETE query building
├─ JOIN support
└─ Subquery support

Replaces: fraiseql/sql/sql_generator.py (633 LOC)
Replaces: fraiseql/sql/query_builder_adapter.py (448 LOC)
```

**Expected Performance**: 10x faster complete query generation

**Validation**: 100% test parity with Python version

---

### Phase C: Operator Migration (Q3-Q4 2026)
**Timeline**: 3-6 months (parallel with Phase B.3)
**Team**: 1-2 engineers
**Status**: Planned

#### C.1: Core Operators (Month 1-2)
```rust
fraiseql_rs/src/operators/
├─ core/string_operators.rs (eq, contains, startswith, etc.)
├─ core/numeric_operators.rs (eq, gt, lt, etc.)
└─ core/boolean_operators.rs (eq, neq)

Replaces: fraiseql/sql/operators/core/*.py (3 files)
```

#### C.2: PostgreSQL Operators (Month 2-3)
```rust
fraiseql_rs/src/operators/
├─ postgresql/network_operators.rs (CIDR, IP, network ops)
├─ postgresql/ltree_operators.rs (tree path operations)
└─ postgresql/daterange_operators.rs (range operations)

Replaces: fraiseql/sql/operators/postgresql/*.py (3 files)
```

#### C.3: Advanced Operators (Month 3-4)
```rust
fraiseql_rs/src/operators/
├─ advanced/jsonb_operators.rs (JSON operations)
├─ advanced/coordinate_operators.rs (GIS operations)
└─ advanced/vector_operators.rs (pgvector operations)

Replaces: fraiseql/sql/operators/advanced/*.py (2 files)
```

#### C.4: Fallback Operators (Month 4-5)
```rust
fraiseql_rs/src/operators/fallback/
├─ comparison_operators.rs
├─ list_operators.rs
├─ null_operators.rs
├─ pattern_operators.rs
└─ ... (generic operators)

Replaces: fraiseql/sql/operators/fallback/*.py (5 files)
```

**Result**: 100% operator coverage in Rust

---

### Phase D: Type Generation (Q1 2027)
**Timeline**: 3-6 months
**Team**: 1 engineer
**Status**: Planned

```rust
fraiseql_rs/src/graphql_generators.rs
├─ generate_where_input_schema(type_def) → schema dict
├─ generate_order_by_input_schema() → schema dict
├─ generate_custom_filter_schema(scalar_type) → schema dict
└─ Support all base + custom types

Replaces: fraiseql/sql/graphql_where_generator.py (1046 LOC)
Replaces: fraiseql/sql/graphql_order_by_generator.py (307 LOC)
```

**Python wrapper** (minimal):
```python
from fraiseql import fraiseql_rs

def create_graphql_where_input(cls: type) -> type:
    schema_dict = fraiseql_rs.generate_where_input_schema(cls)
    return make_dataclass(...)  # Create Python type from schema
```

**User experience**: No changes. Same imports, same decorators.

---

### Phase E: Query Execution (Q2-Q3 2027)
**Timeline**: 3-6 months (optional)
**Team**: 1-2 engineers
**Status**: Optional

```rust
fraiseql_rs/src/executor.rs
├─ execute_query(pool, query_string, params) → Result
├─ Handle connection pooling
├─ Marshal results to Python
└─ Manage transactions

Requires: sqlx or tokio-postgres driver
```

**If implemented**: Queries execute natively in Rust for maximum performance

---

## Detailed Phase Timeline

```
2026 Q1  ├─ Phase A (✅ Complete)
         │  └─ Schema foundation
         │
2026 Q2  ├─ Phase B.1 (WHERE generation)
         │  ├─ Implement query_builder.rs
         │ └─ Create FFI bindings
         │
2026 Q3  ├─ Phase B.2/B.3 (ORDER BY + complete queries)
         ├─ Phase C.1/C.2 (Core + PostgreSQL operators - parallel)
         │
2026 Q4  ├─ Phase C.3/C.4 (Advanced + fallback operators)
         │
2027 Q1  ├─ Phase D (Type generation)
         │  └─ Complete sql/ module replacement
         │
2027 Q2  ├─ Phase E (Optional - query execution)
         │
2027 Q3  └─ v3.0 Release: Rust-only SQL layer
            └─ python sql/ module fully deprecated
```

---

## Migration Strategy: Incremental Replacement

### Approach: Coexistence → Gradual Replacement → Deprecation

```
Year 1: COEXISTENCE
───────────────────────────────────────
Rust Phase B:  WHERE, ORDER BY, queries
Python sql/:   Operators, types (unchanged)
Result:        30-50% of logic in Rust

Migration:     Feature flags allow opt-in
               Code can use both versions

Tests:         100% parity validation


Year 2: GRADUAL REPLACEMENT
───────────────────────────────────────
Rust Phase C:  All operators
Rust Phase D:  Type generation
Python sql/:   Thin compatibility layer only
Result:        90%+ of logic in Rust

Migration:     Operators auto-routed to Rust
               Users see no changes

Tests:         Comprehensive regression suite


Year 3: DEPRECATED
───────────────────────────────────────
Rust Phase E:  Query execution (optional)
Python sql/:   Removed entirely
Result:        100% Rust SQL layer

Migration:     v3.0 release
               Breaking change (intentional)
               Users: Zero changes (Python layer unchanged)

Deployment:    Drop-in replacement for v1.9 / v2.0
```

---

## Success Metrics

### Phase B (Query Building)
- ✅ WHERE clause output matches Python byte-for-byte
- ✅ ORDER BY clause output matches Python byte-for-byte
- ✅ Complete queries generate identical SQL
- ✅ 5-10x performance improvement measured
- ✅ All 383 existing tests pass unchanged
- ✅ No user code changes required

### Phase C (Operators)
- ✅ All 100+ operators implemented
- ✅ Operator output matches Python version
- ✅ Complex nested operations work
- ✅ Edge cases handled
- ✅ Zero regressions in 5000+ operator tests

### Phase D (Type Generation)
- ✅ GraphQL WHERE input types generated correctly
- ✅ GraphQL ORDER BY input types generated correctly
- ✅ Custom filter types supported
- ✅ Type validation works
- ✅ All 17 filter types supported

### Phase E (Query Execution, optional)
- ✅ Queries execute in Rust
- ✅ Results marshaled to Python correctly
- ✅ Performance improvement validated
- ✅ Connection pooling works
- ✅ Transaction support verified

---

## Impact on Different Components

### For Users
```
BEFORE:  import fraiseql
         @fraiseql.type
         class User:
             name: str

         # Query building: Python overhead
         # Performance: Baseline

AFTER:   import fraiseql
         @fraiseql.type
         class User:
             name: str

         # Query building: Rust-powered
         # Performance: 10-100x faster

         # User code: IDENTICAL ✅
```

### For Contributors
```
BEFORE:  Modify operators?        → Edit fraiseql/sql/operators/*.py
         Add new type support?    → Update graphql_where_generator.py
         Performance improve?     → Optimize Python code

AFTER:   Modify operators?        → Edit fraiseql_rs/src/operators/*.rs
         Add new type support?    → Update Rust generators
         Performance improve?     → Rust compiler optimizations

         Python sql/ module:      → Read-only legacy code
```

### For Deployments
```
BEFORE:  Deploy FraiseQL 1.9.5
         Runtime: Python query generation
         Startup: ~100ms schema generation

AFTER:   Deploy FraiseQL 3.0
         Runtime: Rust query generation (10x faster)
         Startup: ~10ms schema generation (cached)

         Compatibility: Drop-in replacement
         Migration effort: 0 hours
```

---

## Risk Mitigation

### Risk 1: Rust FFI Bugs
**Mitigation**:
- Comprehensive test suite (100% parity)
- Gradual rollout with feature flags
- Fallback to Python implementation if needed
- Staged deployment (test → staging → prod)

### Risk 2: Performance Regression
**Mitigation**:
- Benchmark before/after each phase
- Alert on performance decrease
- Revert if needed (version control)
- Keep Python version available

### Risk 3: Compatibility Issues
**Mitigation**:
- Semantic versioning (v3.0 for breaking changes)
- Migration guide provided
- Deprecation period (1-2 years)
- Support both versions if needed

### Risk 4: Rust Code Quality
**Mitigation**:
- Code review process
- Comprehensive testing (property-based if needed)
- Rust linter (clippy) enabled
- Performance profiling included

---

## Resource Requirements

### Development

| Phase | Duration | Engineers | Effort (months) |
|-------|----------|-----------|-----------------|
| A (Done) | 2 weeks | 1 | 0.5 ✅ |
| B | 6-9 mo | 2-3 | 12-18 |
| C | 3-6 mo | 1-2 | 6-12 |
| D | 3-6 mo | 1 | 3-6 |
| E (opt) | 3-6 mo | 1-2 | 3-6 |
| **Total** | 18-24 mo | avg 1.5 | **24-42** |

### Infrastructure
- CI/CD for Rust builds (already have)
- Benchmark infrastructure (have partial)
- Database for integration testing (have)
- Performance monitoring (to be added)

### Documentation
- Implementation guides (per phase)
- Migration guides (for users)
- Rust coding standards (to be defined)
- Architecture documentation (in progress)

---

## Decision: Go or No-Go?

### Reasons to Proceed
✅ Phase A proved foundation works
✅ Performance gains are significant (2.3-4.4x with just caching)
✅ User experience unchanged (Python only)
✅ Rust ecosystem mature for this task
✅ Competitive advantage in performance
✅ Long-term maintainability improved

### Reasons to Hold
❌ Large effort (24-42 person-months)
❌ Risk of regressions despite testing
❌ Current Python version works fine
❌ Resource constraints

### Recommendation
**PROCEED with phased approach** because:
1. Phase A has proven Rust integration works
2. Phased approach reduces risk
3. No breaking changes until v3.0
4. Performance gains justify investment
5. Can pause/resume phases as needed

---

## Next Steps (If Proceeding)

1. **Get approval** for multi-year vision
2. **Plan Phase B** in detail (by month)
3. **Allocate resources** (2-3 engineers)
4. **Set up Rust infrastructure** (more CI/CD, benchmarking)
5. **Begin Phase B.1** in Q2 2026
6. **Validate** against Phase A success criteria

---

*This roadmap envisions a long-term path to a Rust-only SQL layer while maintaining Python-only user experience. It builds on the Phase A foundation proven to work.*

*Success requires commitment, careful migration, and continuous validation.*
