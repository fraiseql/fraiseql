# Architecture Alignment Analysis: Row-Level Auth

**Status**: ⚠️ CRITICAL MISALIGNMENT DETECTED
**Date**: January 4, 2026
**Issue**: Current Python-only implementation contradicts FraiseQL's Python API / Rust Engine architecture

---

## THE ISSUE

Our current implementation puts row-level auth filtering entirely in **Python**:
```
Python (RowFilterResolver + AuthWhereClauseBuilder) → WHERE clause → Rust pipeline
```

But FraiseQL's vision is **Python API / Rust Engine**:
```
Python API (thin) → Rust Engine (heavy lifting) ← Database
```

---

## WHAT RUST ALREADY HAS

Discovered during architecture review:

### 1. **Rust RBAC Module** (`fraiseql_rs/src/rbac/`)
- ✅ `PermissionResolver` - Field-level auth (Rust implementation)
- ✅ `RoleHierarchy` - Role inheritance via PostgreSQL CTEs
- ✅ `FieldAuthChecker` - Pre-execution permission checking
- ✅ `PermissionCache` - LRU in-memory cache + PostgreSQL storage
- ✅ Performance: **<0.1ms cached, <1ms uncached**

### 2. **Python Bindings** (`fraiseql_rs/src/rbac/py_bindings.rs`)
- `PyPermissionResolver` - Python wrapper for Rust resolver
- `PyFieldAuthChecker` - Python wrapper for field auth
- Note: Bindings are **placeholders** ("not yet implemented")

### 3. **What's Missing**
- No row-level WHERE clause filtering in Rust
- Python bindings are incomplete (placeholders only)
- No automatic WHERE injection at Rust level

---

## CORRECT ARCHITECTURE WOULD BE

```
GraphQL Query (Python)
    ↓
Python validates query structure (field names, args)
    ↓
Passes to Rust Pipeline (ALL FILTERING LOGIC)
    ↓
Rust:
  1. Extracts user context from JWT/auth header
  2. Loads role hierarchy + permissions
  3. Applies field-level auth checks
  4. Injects row-level WHERE filters
  5. Executes query on PostgreSQL
    ↓
Results back to Python
    ↓
Python returns to client
```

---

## CURRENT MISALIGNMENT

**Our Implementation (❌ WRONG)**:
```python
# Python middleware
RowLevelAuthMiddleware (Python)
    ↓ (queries database, resolves filters)
RowFilterResolver (Python)
    ↓ (builds WHERE clause)
AuthWhereClauseBuilder (Python)
    ↓ (merges WHERE clauses)
→ Python passes merged WHERE to Rust
```

**Problems**:
1. ❌ **Wrong layer**: Filter resolution happens in Python, not Rust
2. ❌ **Inconsistent**: Field-level auth is in Rust, but row-level is in Python
3. ❌ **Performance**: Makes database queries in Python for filter resolution
4. ❌ **Code duplication**: RBAC logic split between Rust and Python
5. ❌ **Architectural drift**: Contradicts "Python API / Rust Engine" vision

---

## CORRECT ALIGNMENT WOULD BE

**WHERE it should be (✅ RIGHT)**:
```rust
// Rust RBAC module
pub struct RowLevelAuthResolver {
    pub fn get_row_filters(
        user_id: Uuid,
        table_name: &str,
        roles: &[Role],
        context: &AuthContext,
    ) -> Result<Option<WhereClause>> {
        // Query row_constraints table
        // Build WHERE fragments based on roles
        // Return merged WHERE clause
        // All in Rust, all cached, all fast
    }
}

// Python just calls it
result = rust_engine.get_row_filters(user, table, roles)
```

---

## WHAT WE SHOULD DO

### **Option A: Keep Python Implementation (Short-term)**
**Pros**:
- ✅ Quick to implement (we're 50% done)
- ✅ Functional for v1.9.1
- ✅ Proves the concept works

**Cons**:
- ❌ Architecturally misaligned
- ❌ Performance penalty (queries from Python)
- ❌ Maintenance burden (RBAC logic split)
- ❌ Code duplication with Rust RBAC

**Use case**: Quick security fix if row-level auth is blocking release

---

### **Option B: Refactor to Rust (Correct)**
**Pros**:
- ✅ Architecturally aligned with Python API / Rust Engine
- ✅ Consistent: All RBAC logic in Rust
- ✅ Better performance: No Python overhead
- ✅ Future-proof: Easier to extend

**Cons**:
- ❌ Requires Rust implementation
- ❌ Longer timeline (2-3 weeks for full implementation)
- ❌ May delay v1.9.1 release

**Use case**: Production-grade implementation aligned with vision

---

## RECOMMENDATION

### **Pragmatic Hybrid Approach (RECOMMENDED)**

1. **Phase 1-3: Keep Python implementation** (DONE ✓)
   - Completes row-level auth for v1.9.1
   - Fixes security gap in framework review
   - Keeps release on schedule

2. **Phase 4-5: Mark as "v1.9.1 temporary"**
   - Document that Python layer is interim
   - Add TODO comments pointing to future Rust implementation
   - Create architectural plan for Rust refactor

3. **Phase 6+: Refactor to Rust** (Post v1.9.1)
   - Move RowFilterResolver logic to Rust
   - Extend Rust PermissionResolver with row-level filters
   - Complete Python bindings for field auth
   - Performance: <0.1ms overhead (Rust vs Python ~1ms)

---

## WHAT NEEDS TO CHANGE IN OUR CURRENT CODE

### In RowFilterResolver (Python):

Add architectural warning:

```python
"""Row-Level Access Filter Resolution

⚠️  TEMPORARY PYTHON IMPLEMENTATION

This module is in Python for v1.9.1 deadline. It should be
refactored to Rust as part of the RBAC unification effort.

Current architecture (❌):
  RowFilterResolver (Python) → Rust pipeline

Desired architecture (✅):
  Rust RBAC Module → Row-level filters + field auth in Rust
  Python just calls Rust bindings

Timeline: Rust refactor planned for v1.10 or v2.0

See: .phases/ISSUE-2-ARCHITECTURE-ALIGNMENT-ANALYSIS.md
"""
```

### Integration points:

```python
# In graphql_type.py resolver:
# TEMPORARY: Call Python row filter resolver
row_filters = await python_row_filter_resolver.get_filters(...)

# FUTURE: Call Rust implementation directly
# row_filters = rust_engine.rbac.get_row_filters(...)
```

---

## LONGER-TERM ARCHITECTURE VISION

### What Rust RBAC should handle (unified):

1. **Field-level authorization** (currently in Rust ✓)
   - Checks which fields user can see
   - Pre-execution validation

2. **Row-level filtering** (should be in Rust, currently in Python ❌)
   - Checks which rows user can access
   - Builds WHERE clause filters
   - Merges with explicit WHERE clauses

3. **Caching** (currently in Rust ✓)
   - LRU in-memory cache
   - PostgreSQL persistence
   - Automatic invalidation

### Python API layer would:
- Validate query structure
- Call Rust RBAC methods
- Handle GraphQL response formatting
- Return to client

---

## DECISION NEEDED

### For v1.9.1:
Should we:

**A) Continue with Python implementation** (current approach)
   - Finish Phases 4-5 with Python code
   - Mark as "interim for v1.9.1"
   - Plan Rust refactor for next version

**B) Stop and refactor to Rust** (correct long-term)
   - Implement in Rust RBAC module
   - Delays release by 1-2 weeks
   - Architecturally correct from day one

**C) Hybrid approach** (recommended)
   - Continue Python for quick v1.9.1 win
   - Add clear "refactor to Rust" comments
   - Create detailed Rust implementation plan
   - Do refactor in v1.10/v2.0

---

## COMMITMENT

**If continuing with Python**:
- Add architectural warnings to all files
- Document this as "temporary for v1.9.1"
- Create detailed Rust refactor plan
- Commit to refactoring in next version

**If refactoring to Rust**:
- Revert our Python implementation
- Extend Rust RBAC module instead
- Implement Python bindings
- Takes 2-3 weeks but architecturally correct

---

## CONCLUSION

Our current implementation is **functionally correct** but **architecturally misaligned**.

The real question: **Do we want quick (Python) or correct (Rust)?**

**My recommendation**: Continue Python for v1.9.1 (time-constrained), but clearly mark it as interim and plan Rust refactor. This balances security fix + release timeline + long-term architecture.
