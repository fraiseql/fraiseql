# Vision: Rust-Only SQL Layer (Long-Term Architecture)

**Date**: January 8, 2026
**Status**: Strategic Vision / Architecture Planning
**Target**: FraiseQL v2.0+ (post-Phase A)

---

## Executive Summary

**YES, it is possible** to move the entire sql/ module to Rust while keeping users writing Python-only code.

This document outlines a long-term vision for a **Rust-only SQL layer** that eliminates the Python sql/ module entirely, replacing it with Rust-based query generation accessed via FFI.

**Key Principle**: Users only write Python. The entire query execution pipeline moves to Rust.

---

## Current State (Phase A Complete)

### What's Currently in Python (sql/ module)

```
sql/
├── graphql_where_generator.py    (1046 lines) - Generates GraphQL filter types
├── graphql_order_by_generator.py (307 lines)  - Generates GraphQL order types
├── where_generator.py            (480 lines)  - WHERE clause generation
├── order_by_generator.py         (240 lines)  - ORDER BY generation
├── sql_generator.py              (633 lines)  - SQL building
├── query_builder_adapter.py      (448 lines)  - Query adapter
├── operators/                    (30 files)   - Operator implementations
└── where/                        (40 files)   - WHERE clause builders
```

**Total: ~3,500 lines of Python SQL logic**

### What's Currently in Rust

```
fraiseql_rs/
├── src/schema_generators.rs      (130 lines) - Schema export (Phase A.1)
└── ... (existing Rust code for JSON transformation, etc.)
```

### Dependencies on sql/ from Framework

```
Core Framework (11 modules):
├── cqrs/repository.py         - Uses where_generator
├── db/repository.py           - Uses where_generator
├── db/query_builder.py        - Uses SQL generation
├── gql/builders/query_builder.py - Uses GraphQL type generators
├── core/graphql_type.py       - Uses GraphQL type generators
└── ... (5 more)

Total: 70 external imports of sql/
```

---

## Long-Term Vision: The Rust-Only Approach

### Phase B: Move Query Building to Rust (Months 6-9)

**Objective**: Migrate `sql_generator.py` and `query_builder_adapter.py` to Rust

```
Current Flow:
┌──────────────┐
│ Python API   │
├──────────────┤
│ sql/         │ ← To be replaced
│  - WHERE gen │
│  - ORDER gen │
│  - SQL gen   │
├──────────────┤
│ Database     │
└──────────────┘

Target Flow:
┌──────────────┐
│ Python API   │ (User only writes this)
├──────────────┤
│  FFI calls   │
├──────────────┤
│ Rust Pipeline│ (WHERE, ORDER, SQL generation)
│ fraiseql_rs/ │
├──────────────┤
│ Database     │
└──────────────┘
```

**Implementation Strategy**:

1. **Create `fraiseql_rs/src/query_builder.rs`**
   ```rust
   pub fn build_where_clause(
       filters: HashMap<String, Value>,
       schema: &FilterSchema
   ) -> Result<String, QueryError> {
       // Replaces: sql/where_generator.py
       // Replaces: sql/where/core/sql_builder.py
       // Returns: SQL WHERE clause string
   }

   pub fn build_order_by_clause(
       ordering: Vec<OrderSpec>,
       schema: &OrderBySchema
   ) -> Result<String, QueryError> {
       // Replaces: sql/order_by_generator.py
       // Returns: SQL ORDER BY clause string
   }

   pub fn build_query(
       query_spec: QuerySpecification
   ) -> Result<SqlQuery, QueryError> {
       // Replaces: sql/sql_generator.py
       // Returns: Complete SELECT/UPDATE/DELETE query
   }
   ```

2. **Create Python FFI layer in `fraiseql_rs` lib**
   ```python
   # In fraiseql_rs (Rust-Python bridge)
   def build_where_clause(filters, schema) -> str
   def build_order_by_clause(ordering, schema) -> str
   def build_query(spec) -> Query
   ```

3. **Update `fraiseql/db/repository.py`**
   ```python
   # OLD:
   from fraiseql.sql.where_generator import build_where
   where_sql = build_where(filters)

   # NEW:
   from fraiseql import fraiseql_rs
   where_sql = fraiseql_rs.build_where_clause(filters, schema)
   ```

### Phase C: Move Operator Logic to Rust (Months 9-12)

**Objective**: Migrate all operator implementations

```
Current:
fraiseql/sql/operators/ (30 Python files)
  ├── core/ (string, numeric, boolean)
  ├── postgresql/ (network, ltree, daterange)
  ├── advanced/ (coordinate, jsonb)
  └── fallback/ (generic operators)

Target:
fraiseql_rs/src/operators/ (same structure in Rust)
  ├── core/ (string, numeric, boolean)
  ├── postgresql/ (network, ltree, daterange)
  ├── advanced/ (coordinate, jsonb)
  └── fallback/ (generic operators)
```

**Each operator becomes a Rust function**:
```rust
pub fn apply_string_eq(value: &str, filter: &str) -> SqlCondition
pub fn apply_string_contains(value: &str, pattern: &str) -> SqlCondition
pub fn apply_numeric_gt(value: Value, threshold: Value) -> SqlCondition
// ... 100+ operators
```

### Phase D: Move Type Generation to Rust (Months 12-15)

**Objective**: Move GraphQL type generation to Rust

```
Current:
fraiseql/sql/graphql_where_generator.py (1046 lines)
fraiseql/sql/graphql_order_by_generator.py (307 lines)

Target:
fraiseql_rs/src/graphql_generators.rs
  ├── generate_where_input_type()
  ├── generate_order_by_input_type()
  └── generate_custom_filter_type()
```

**Python side becomes minimal wrapper**:
```python
from fraiseql import fraiseql_rs

def create_graphql_where_input(cls: type) -> type:
    # Call Rust to generate type schema
    schema_dict = fraiseql_rs.generate_where_input_schema(cls)

    # Create Python dataclass from schema
    return make_dataclass(...)
```

### Phase E: SQL Execution (Optional, Months 15+)

**Objective**: Optional - Move query execution itself to Rust

This would require a Rust database driver (sqlx, tokio-postgres, etc.)

```rust
pub async fn execute_query(
    pool: &PgPool,
    query: &str,
    params: &[Value]
) -> Result<Vec<Row>, DbError>
```

---

## Detailed Roadmap

### Pre-Requisites (Complete ✅)
- ✅ Phase A: Schema export infrastructure
- ✅ Phase A: Python schema loader with caching
- ✅ Phase A: Integration with WHERE/OrderBy generators

### Phase B: Query Building Foundation
**6-9 months**

1. **Milestone B.1**: WHERE clause generation in Rust
   - Implement basic WHERE clause building
   - Port all operator strategies
   - Create comprehensive test suite
   - Verify SQL output matches Python version

2. **Milestone B.2**: ORDER BY clause generation in Rust
   - Implement ORDER BY building
   - Handle nested orderings
   - Merge with WHERE clause generation

3. **Milestone B.3**: Complete query building
   - Add SELECT/UPDATE/DELETE support
   - Handle joins and subqueries
   - Create FFI bindings

### Phase C: Operator Migration
**9-12 months**

1. **Milestone C.1**: Core operators (string, numeric, boolean)
2. **Milestone C.2**: PostgreSQL operators (network, ltree, daterange)
3. **Milestone C.3**: Advanced operators (coordinate, jsonb, vector)
4. **Milestone C.4**: Fallback operators (generic, pattern, null)

### Phase D: Type Generation
**12-15 months**

1. **Milestone D.1**: GraphQL WHERE input generation
2. **Milestone D.2**: GraphQL OrderBy input generation
3. **Milestone D.3**: Custom scalar filter generation

### Phase E: Execution Layer (Optional)
**15+ months**

1. Query execution in Rust
2. Connection pooling in Rust
3. Result marshaling to Python

---

## Benefits of Rust-Only SQL Layer

### Performance
- **10-100x faster** query building (no Python overhead)
- **Zero-copy** data structures
- **Native parallelization** possible
- **Memory efficient** - Rust's ownership model

### Reliability
- **Type-safe** at compile time
- **No runtime type errors** in query building
- **Predictable memory** usage
- **Fearless concurrency**

### Maintainability
- **Single implementation** (not Python + Rust)
- **Unified test suite**
- **Consistent operator behavior**
- **Easier to reason about** query semantics

### User Experience
- **No changes** - users still write Python only
- **Transparent optimization** - faster queries without code changes
- **Better error messages** - compile-time checks catch issues earlier
- **Automatic updates** - Rust compiler catches breaking changes

---

## User Experience: No Changes!

### Users Still Write This (Python)

```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
    age: int

@fraiseql.query
async def get_users(
    info,
    where: UserWhereInput | None = None,
    order_by: UserOrderByInput | None = None
) -> list[User]:
    return await info.context["db"].find("users", where=where, order_by=order_by)

# Usage still the same:
users = await get_users(
    info,
    where=UserWhereInput(name=StringFilter(contains="john")),
    order_by=UserOrderByInput(created_at=OrderDirection.DESC)
)
```

### But Behind the Scenes (Rust-Powered)

```
1. UserWhereInput schema from Phase A.1 (Rust export) ✓
2. Filter validation via Phase A.2 (Rust schema loader) ✓
3. WHERE clause building via Phase B (Rust query builder) ← NEW
4. Operator execution via Phase C (Rust operators) ← NEW
5. Type generation via Phase D (Rust type generator) ← NEW
```

---

## Migration Strategy

### Strategy 1: Incremental Replacement (Recommended)
1. Keep Python sql/ during transition
2. Add Rust alternatives alongside
3. Gradually route code to Rust version
4. Remove Python version when confident

**Timeline**: 15-18 months (careful, validated approach)
**Risk**: Low (backward compatible)
**Effort**: Medium (need to maintain both versions temporarily)

### Strategy 2: Big Bang Replacement
1. Complete rewrite of sql/ in Rust
2. Ship as breaking change (v2.0)
3. No Python version

**Timeline**: 12-15 months (faster)
**Risk**: High (potential bugs in complete rewrite)
**Effort**: High (all at once)

### Strategy 3: Hybrid Approach (Best)
1. Implement high-value items first (WHERE, ORDER BY generation)
2. Keep operators in Python initially
3. Migrate operator implementations one-by-one
4. Phase out sql/ module gradually

**Timeline**: 18-24 months (balanced)
**Risk**: Low (risk mitigation as we go)
**Effort**: Medium (steady progress)

---

## Technical Challenges and Solutions

### Challenge 1: Operator Extensibility
**Problem**: Users might add custom operators

**Solution**:
- Provide Rust trait for custom operators
- Allow users to define operators as Rust code
- Or keep Python hook for custom operators
- Plugin system via FFI

### Challenge 2: Type System Mismatch
**Problem**: Python's dynamic typing vs Rust's static typing

**Solution**:
- JSON schema as intermediate representation (Phase A already does this!)
- Type information flows through schema
- Rust validates against schema at runtime
- Python receives validated results

### Challenge 3: Error Messages
**Problem**: Errors from Rust might be less helpful in Python context

**Solution**:
- Rust provides detailed error information
- Python formats errors with context
- Stack traces preserved through FFI
- Better error types with line numbers

### Challenge 4: Migration Compatibility
**Problem**: Code might break during transition

**Solution**:
- Feature flags (Rust implementation optional initially)
- Comprehensive test suite
- Validate Rust output matches Python
- Gradual rollout (some queries use Rust, others Python)

---

## Validation Strategy

### Each Phase Requires
1. **100% test parity**: Rust output matches Python exactly
2. **Performance benchmarks**: Rust is faster by measurable margin
3. **Regression tests**: No functionality lost
4. **User validation**: Real queries tested
5. **Documentation**: How Rust changes map to Python

### Example Validation (Phase B)
```python
def test_where_clause_parity():
    """Rust WHERE generation matches Python"""

    # Python version
    python_where = sql.build_where_clause(filters)

    # Rust version
    rust_where = fraiseql_rs.build_where_clause(filters, schema)

    # Should generate identical SQL
    assert parse_sql(python_where) == parse_sql(rust_where)

    # Rust should be faster
    assert rust_time < python_time * 0.5  # 2x+ faster
```

---

## Long-Term Vision (3-5 years)

### FraiseQL v3.0: Fully Rust SQL Layer

```
User Code (Python):
  ↓
Type Annotations & Decorators
  ↓
Compiled to GraphQL Schema
  ↓
FFI Boundary (fraiseql_rs)
  ↓
Rust SQL Layer:
  ├─ Schema validation
  ├─ Query building
  ├─ Operator execution
  ├─ Type generation
  └─ (Optional: Database execution)
  ↓
PostgreSQL Database
```

### Possible Extensions
- **Distributed query execution** - Rust enables concurrency
- **Query optimization** - Rust can compile/cache query plans
- **Advanced caching** - Rust-level query result caching
- **Real-time subscriptions** - Native async in Rust
- **GraphQL federation** - Efficient upstream/downstream translation

---

## Investment Required

### Development Effort
- Phase B (Query Building): 6-9 months (2-3 engineers)
- Phase C (Operators): 3-6 months (1-2 engineers)
- Phase D (Type Generation): 3-6 months (1 engineer)
- Phase E (Execution): 3-6 months (1-2 engineers)

**Total: 15-27 person-months** (approximately 2-3 years with 1-2 engineers)

### Benefits Realization
- **Year 1**: 2-3x faster query building
- **Year 2**: 5-10x faster query building + operators
- **Year 3**: 10-100x faster with full Rust layer

---

## Decision Points

**Question 1**: Do we want a fully Rust SQL layer?
- ✅ YES: Proceed with phased approach
- ❌ NO: Keep Python sql/, just optimize critical paths

**Question 2**: Should users be able to write custom operators?
- ✅ YES: Design Rust plugin system
- ❌ NO: Fixed set of operators only

**Question 3**: Do we want query execution in Rust?
- ✅ YES: Phase E becomes mandatory
- ❌ NO: Stop at query building (Phase B/C complete)

---

## Summary

**Feasibility: YES ✅**

A Rust-only SQL layer is achievable with:
1. Clear phased approach
2. Continuous validation
3. User experience unchanged
4. 15-27 month timeline
5. Significant performance gains

**Phase A has proven** the foundation works:
- Schema export from Rust ✓
- FFI integration ✓
- Caching strategy ✓
- Performance improvement ✓

**Next steps** to pursue this vision:
1. Get stakeholder buy-in on phased approach
2. Plan Phase B in detail
3. Allocate engineering resources
4. Begin migration in Q3 2026

---

*This vision document outlines a path to eliminate the Python sql/ module entirely while maintaining Python-only user experience.*

*Success depends on careful migration, continuous validation, and commitment to the long-term goal.*
