# Issue #2: Row-Level Authorization - Rust-First Implementation Plan

**Status**: Architecture-Aligned Design Phase
**Decision**: Implement row-level filtering in Rust RBAC module (not Python interim)
**Approach**: Extend existing Rust infrastructure for unified RBAC (field-level + row-level)
**Duration**: 3-4 weeks for complete implementation
**Target**: Production-grade, architecturally correct solution

---

## EXECUTIVE SUMMARY

FraiseQL's vision: **Python API / Rust Engine architecture**

Your decision: Implement row-level authorization correctly from day one in the Rust engine, not as a Python interim.

**Why this is right:**
- ✅ Architecturally aligned with stated vision
- ✅ All RBAC logic unified in Rust (field-level + row-level + caching)
- ✅ Better long-term performance (<0.1ms vs ~1ms Python)
- ✅ No code duplication or architectural debt
- ✅ Python API simply calls Rust implementations

**Cost:** 3-4 week development timeline. But this is the correct solution that will serve the framework for years.

---

## CURRENT STATE: Rust RBAC Infrastructure

Existing implementations (already in production):

### ✅ **PermissionResolver** (`fraiseql_rs/src/rbac/resolver.rs`)
- Field-level permission checking
- Multi-layer caching (LRU + PostgreSQL)
- Role hierarchy traversal via CTEs
- Performance: <0.1ms cached, <1ms uncached
- Thread-safe, multi-tenant aware

### ✅ **RoleHierarchy** (`fraiseql_rs/src/rbac/hierarchy.rs`)
- PostgreSQL recursive CTEs for role inheritance
- Computed in <2ms
- Full tenant isolation

### ✅ **PermissionCache** (`fraiseql_rs/src/rbac/cache.rs`)
- LRU in-memory cache
- PostgreSQL persistence
- TTL expiry handling
- Thread-safe via Mutex

### ✅ **FieldAuthChecker** (`fraiseql_rs/src/rbac/field_auth.rs`)
- Pre-execution field permission validation
- Integrates with PermissionResolver
- GraphQL directive framework ready

### ⚠️ **Python Bindings** (`fraiseql_rs/src/rbac/py_bindings.rs`)
- PyPermissionResolver: Wrapper exists, async methods are placeholders
- PyFieldAuthChecker: Placeholder implementation
- **Ready to be completed** with full async Python integration

---

## WHAT NEEDS TO BE ADDED

### 1. **Row-Level Constraint Resolver** (NEW - Rust)
**File**: `fraiseql_rs/src/rbac/row_constraints.rs` (~300 LOC)

Purpose: Query and evaluate row-level constraints from database

```rust
pub struct RowConstraintResolver {
    pool: Pool,
    cache: Arc<ConstraintCache>,  // Similar to PermissionCache
}

impl RowConstraintResolver {
    /// Get WHERE clause filter for user access to table rows
    pub async fn get_row_filters(
        &self,
        user_id: Uuid,
        table_name: &str,
        roles: &[Role],
        tenant_id: Option<Uuid>,
    ) -> Result<Option<RowFilter>> {
        // Returns: None (no filter), or RowFilter with field+value
        // Example: RowFilter { field: "tenant_id", value: user_tenant_id }
    }

    /// Evaluate complex constraint expressions
    fn evaluate_constraint_expression(
        expression: &str,
        user_context: &UserContext,
    ) -> Result<bool> {
        // Support templated expressions like:
        // "status = 'published' AND tenant_id = :user_tenant_id"
    }
}
```

### 2. **WHERE Clause Merging** (NEW - Rust)
**File**: `fraiseql_rs/src/rbac/where_merger.rs` (~150 LOC)

Purpose: Safely merge explicit GraphQL WHERE clauses with auth-injected filters

```rust
pub struct WhereClauseMerger;

impl WhereClauseMerger {
    /// Merge explicit WHERE with auth filters
    pub fn merge_where_clauses(
        explicit_where: Option<&JsonValue>,
        auth_filter: Option<&RowFilter>,
    ) -> Result<Option<JsonValue>> {
        // Handles AND composition, conflict detection, etc.
        // Returns merged WHERE clause safe for execution
    }

    /// Detect conflicts between explicit and auth filters
    pub fn detect_conflicts(
        explicit_where: &JsonValue,
        auth_filter: &RowFilter,
    ) -> Vec<Conflict> {
        // Identifies field-level conflicts
    }
}
```

### 3. **Database Schema** (SQL)
**File**: `fraiseql_rs/migrations/row_constraints.sql`

```sql
CREATE TABLE row_constraints (
    id UUID PRIMARY KEY,
    table_name VARCHAR,
    role_id UUID,
    constraint_type VARCHAR,  -- 'ownership', 'tenant', 'expression'
    field_name VARCHAR,       -- 'owner_id', 'tenant_id'
    expression VARCHAR,       -- Custom SQL expression
    UNIQUE(table_name, role_id, constraint_type)
);
```

### 4. **Python Bindings - Complete Async Implementation**
**File**: `fraiseql_rs/src/rbac/py_bindings.rs` (extend existing)

Complete the placeholder methods:

```rust
#[pymethods]
impl PyPermissionResolver {
    /// Async method: Get user permissions
    pub fn get_user_permissions_async(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
        py: Python,
    ) -> PyResult<PyObject> {
        // Return coroutine that Python can await
    }

    /// Async method: Check permission
    pub fn has_permission_async(...) -> PyResult<PyObject> { ... }
}

#[pyclass]
pub struct PyRowConstraintResolver {
    resolver: Arc<RowConstraintResolver>,
}

#[pymethods]
impl PyRowConstraintResolver {
    /// Get row filters for user on table
    pub fn get_row_filters_async(...) -> PyResult<PyObject> { ... }
}
```

### 5. **Python Integration Layer** (THIN - Python)
**File**: `src/fraiseql/enterprise/rbac/row_constraints_integration.py` (~100 LOC)

Thin wrapper that calls Rust bindings:

```python
from fraiseql._fraiseql_rs import PyRowConstraintResolver

class RowConstraintProxy:
    """Thin Python wrapper around Rust row constraint resolver"""

    def __init__(self, rust_pool):
        self._resolver = PyRowConstraintResolver(rust_pool)

    async def get_row_filters(self, user_id, table, roles, context):
        # Call Rust async method
        result = await self._resolver.get_row_filters_async(
            str(user_id), table, str(context.get('tenant_id'))
        )
        return result
```

### 6. **GraphQL Middleware** (Python - Uses Rust)
**File**: `src/fraiseql/enterprise/rbac/row_level_auth_middleware.py` (~200 LOC)

Orchestrates Rust row constraints in GraphQL pipeline:

```python
class RowLevelAuthMiddleware:
    """Middleware that uses Rust row constraint resolver"""

    def __init__(self, row_constraints_proxy):
        self.constraints = row_constraints_proxy

    async def resolve(self, next_, root, info, **kwargs):
        if root is not None:  # Only root level
            return await next_(root, info, **kwargs)

        # Get row filters from RUST via proxy
        filters = await self.constraints.get_row_filters(
            info.context.get('user_id'),
            self._detect_table(info),
            info.context.get('roles'),
            info.context
        )

        # Store for resolver to use
        info.context['__row_level_filters__'] = filters

        return await next_(root, info, **kwargs)
```

### 7. **Resolver Integration** (Python - minimal change)
**File**: `src/fraiseql/core/graphql_type.py` (~20 LOC change)

Where the WHERE clause merging happens:

```python
async def resolve_list_query(info, where=None, **kwargs):
    explicit_where = where or {}

    # Get filters from middleware context (computed in Rust)
    row_filters = info.context.get('__row_level_filters__', {}).get(table_name)

    # Merge using Rust logic (via Python binding)
    if row_filters:
        from fraiseql._fraiseql_rs import PyWhereClauseMerger
        merger = PyWhereClauseMerger()
        merged_where = merger.merge(explicit_where, row_filters)
    else:
        merged_where = explicit_where

    # Execute via standard Rust pipeline
    where_clause = normalize_dict_where(merged_where, table_name)
    return await execute_via_rust_pipeline(where_clause)
```

---

## IMPLEMENTATION PHASES

### **Phase 1: Rust Row Constraint Resolver** (1 week)
- Create `row_constraints.rs` module
- Implement `RowConstraintResolver` struct
- Add constraint querying logic
- Add caching layer (reuse PermissionCache pattern)
- Comprehensive Rust unit tests

**Deliverables:**
- ✅ Row constraint resolver fully functional
- ✅ Database queries optimized
- ✅ Caching working
- ✅ 100% Rust test coverage

### **Phase 2: WHERE Clause Merging** (3-4 days)
- Create `where_merger.rs` module
- Implement merge logic (AND composition)
- Implement conflict detection
- Rust unit tests

**Deliverables:**
- ✅ WHERE merging logic complete
- ✅ Conflict detection working
- ✅ Edge cases handled (None, empty, nested)
- ✅ 100% test coverage

### **Phase 3: Python Async Bindings** (4-5 days)
- Extend `py_bindings.rs` with async support
- Use `pyo3_asyncio` for async Python bindings
- Implement PyRowConstraintResolver
- Implement PyWhereClauseMerger
- Python integration tests

**Deliverables:**
- ✅ Python can call Rust async methods
- ✅ All bindings awaitable from Python
- ✅ Error handling working
- ✅ Performance benchmarks showing <0.1ms overhead

### **Phase 4: Middleware & Integration** (3-4 days)
- Create `row_level_auth_middleware.py`
- Create `row_constraints_integration.py`
- Integrate with existing RbacMiddleware
- Register in app setup
- Integration tests with real GraphQL queries

**Deliverables:**
- ✅ Middleware working end-to-end
- ✅ Filters automatically applied
- ✅ Works with RbacMiddleware
- ✅ Zero data exposure confirmed

### **Phase 5: Database Schema & Migration** (1-2 days)
- Create migration script for `row_constraints` table
- Create indices for fast lookups
- Document schema
- Provide example constraint configurations

**Deliverables:**
- ✅ Schema migration complete
- ✅ Indices optimal
- ✅ Documentation clear

### **Phase 6: Comprehensive Testing** (2-3 days)
- Unit tests for all Rust components
- Integration tests (GraphQL + database)
- Security tests (bypass prevention)
- Performance benchmarks
- Documentation

**Deliverables:**
- ✅ 100% test coverage
- ✅ Zero regressions
- ✅ Performance targets met
- ✅ Security validated

---

## ARCHITECTURE DIAGRAM

```
GraphQL Query (Python)
    ↓
RbacMiddleware
  └─ Extract user context
  └─ Set in info.context
    ↓
RowLevelAuthMiddleware (Python calls Rust)
  ├─ Detect table being queried
  └─ Call Rust RowConstraintResolver
      ├─ Query row_constraints table (PostgreSQL)
      ├─ Evaluate constraint for user's roles
      ├─ Cache result (LRU + PostgreSQL)
      └─ Return RowFilter (e.g., {tenant_id: user_tenant_id})
    ↓
    Store filter in context
    ↓
GraphQL Field Resolver (Python)
  ├─ Get explicit WHERE from args
  ├─ Get filter from context (set by middleware)
  └─ Call Rust WhereClauseMerger
      └─ Merge WHERE + auth filter
      └─ Return merged WHERE (with AND)
    ↓
normalize_dict_where() + execute_via_rust_pipeline()
    ↓
PostgreSQL query with merged WHERE
    ↓
Results back to client
```

---

## KEY DESIGN DECISIONS

### ✅ **All RBAC Logic in Rust**
- Field-level auth: Already in Rust ✓
- Row-level filtering: NEW in Rust
- WHERE merging: NEW in Rust
- Caching: Already in Rust ✓

### ✅ **Python API Stays Thin**
- Just middleware orchestration
- Just calls Rust methods
- No business logic in Python

### ✅ **Performance Target: <0.1ms Overhead**
- Cached row constraints: <0.1ms (LRU hit)
- Uncached: <1ms (PostgreSQL query)
- WHERE merging: <0.05ms (Rust JSON)
- Total: Minimal overhead

### ✅ **Multi-Tenant Safe**
- All constraint queries filtered by tenant_id
- Cache keys include tenant_id
- Hierarchy respects tenant boundaries

### ✅ **Backward Compatible**
- Existing code works unchanged
- Row constraints optional (opt-in per table)
- Graceful degradation (no filter = no WHERE injection)

---

## DATA FLOW EXAMPLE

**Scenario**: User with "user" role queries their documents

```
Input:
  user_id: "550e8400-e29b-41d4-a716-446655440000"
  table: "documents"
  roles: [Role(id="...", name="user")]
  tenant_id: "tenant-123"
  GraphQL WHERE: {status: {eq: "published"}}

Row Constraint Resolver (Rust):
  1. Check cache: MISS
  2. Query database:
     SELECT * FROM row_constraints
     WHERE table_name='documents' AND role_id IN (SELECT id FROM roles WHERE name='user')
  3. Found: constraint_type='ownership', field_name='owner_id'
  4. Build filter: {owner_id: {eq: user_id}}
  5. Cache result with 5m TTL
  6. Return: RowFilter { field: "owner_id", value: user_id }

WHERE Merger (Rust):
  1. Explicit WHERE: {status: {eq: "published"}}
  2. Auth filter: {owner_id: {eq: user_id}}
  3. Merge with AND:
     {AND: [
       {status: {eq: "published"}},
       {owner_id: {eq: user_id}}
     ]}
  4. Return merged WHERE

Execute (Python/Rust Pipeline):
  1. normalize_dict_where() converts to WhereClause
  2. execute_via_rust_pipeline() executes query
  3. SQL: SELECT * FROM documents
           WHERE status = 'published' AND owner_id = $1

Result: Only user's published documents returned ✓
```

---

## TESTING STRATEGY

### **Unit Tests (Rust)**
- Row constraint querying
- Constraint evaluation (ownership, tenant, expression)
- WHERE clause merging
- Conflict detection
- Caching behavior
- Performance benchmarks

### **Integration Tests (Python + Rust)**
- GraphQL queries with row filtering
- Nested queries (documents + comments)
- Mutations with filtering
- Real database interaction
- Middleware orchestration

### **Security Tests**
- Explicit WHERE bypass prevention
- NULL filter abuse prevention
- Permission escalation detection
- Tenant isolation verification
- Role hierarchy correctness

### **Performance Benchmarks**
- Cached constraint resolution <0.1ms
- Uncached constraint resolution <1ms
- WHERE merging <0.05ms
- End-to-end overhead <1.5ms

---

## TIMELINE

| Phase | Task | Duration |
|-------|------|----------|
| 1 | Rust row constraint resolver | 1 week |
| 2 | WHERE clause merging (Rust) | 3-4 days |
| 3 | Python async bindings | 4-5 days |
| 4 | Middleware & integration | 3-4 days |
| 5 | Database schema & migration | 1-2 days |
| 6 | Testing & documentation | 2-3 days |
| **Total** | | **3-4 weeks** |

---

## WHY THIS IS THE RIGHT CHOICE

1. **Architectural Correctness**
   - Aligns with "Python API / Rust Engine" vision
   - All RBAC logic unified in Rust
   - No code duplication
   - No architectural debt

2. **Long-Term Maintainability**
   - Single source of truth (Rust RBAC module)
   - Easier to extend (new constraint types, complex logic)
   - Consistent behavior across field + row level auth
   - No Python-Rust logic divergence

3. **Performance**
   - <0.1ms cached (vs ~1ms Python)
   - 10x faster constraint evaluation
   - Better caching (centralized)
   - Scales to millions of constraints

4. **Production Quality**
   - Proper error handling from day one
   - Thread-safe Rust implementation
   - Multi-tenant safe
   - Audit logging framework ready

5. **Zero Technical Debt**
   - No "temporary Python interim" to migrate later
   - No regret-purchase of refactoring cost
   - No performance cliff when scaling

---

## SUCCESS CRITERIA

✅ Row-level filters automatically applied to GraphQL queries
✅ Explicit WHERE clauses safely merged with auth filters
✅ <0.1ms performance overhead (cached)
✅ <1ms performance overhead (uncached)
✅ 100% backward compatible
✅ Zero data exposure vulnerabilities
✅ 100% test coverage
✅ Multi-tenant safe
✅ Documentented with examples

---

## NEXT STEPS

1. ✅ Decision made: Rust-first implementation (perfectionist approach)
2. ⏳ Create Phase 1: Row constraint resolver (Rust)
3. ⏳ Implement Phase 1-6 sequentially
4. ⏳ Full test suite as we go
5. ⏳ Create PR with all Rust + Python changes
6. ⏳ Code review with architecture focus

---

**This is the right solution. It takes longer, but it's architecturally correct and will serve FraiseQL excellently for years to come.**
