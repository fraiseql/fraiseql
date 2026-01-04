# Phase 4: Middleware & Integration - Row-Level Authorization

**Status**: Planning
**Issue**: #2 - Row-Level Authorization Middleware
**Target**: Integrate Rust row constraint resolution into GraphQL middleware and resolvers

## Overview

Phase 4 integrates the Rust components (RowConstraintResolver, WhereMerger) created in Phases 1-3 into the existing Python middleware and GraphQL execution pipeline.

**Key Deliverables**:
1. Extend existing `RbacMiddleware` with row-level constraint resolution
2. Create `RowConstraintResolver` wrapper (Python → Rust)
3. Create `WhereMerger` wrapper (Python → Rust)
4. Integrate WHERE clause merging into query resolution
5. Add row-level filtering to GraphQL context

## Architecture

### Current Flow (Permission-Level Only)
```
GraphQL Request
    ↓
RbacMiddleware (extracts user/tenant context)
    ↓
PermissionResolver (checks field-level permissions)
    ↓
GraphQL Execution (applies WHERE clause from user input)
    ↓
Database Query
    ↓
Results (no row-level filtering applied)
```

### Target Flow (With Row-Level Authorization)
```
GraphQL Request
    ↓
RbacMiddleware (extracts user/tenant context)
    ├─ Permission checks (existing)
    └─ Row-level filters (NEW: Phase 4)
        ├ RowConstraintResolver.get_row_filters()
        └ Build row-level WHERE fragment
    ↓
Query Resolution (existing)
    ├ Extract explicit WHERE from GraphQL args
    ├ Merge with row-level filter (WhereMerger)
    └ Ensure conflicts trigger proper error handling
    ↓
Database Query (includes row-level WHERE)
    ↓
Filtered Results (only accessible rows)
```

## Implementation Plan

### 1. Create `RustRowConstraintResolver` Wrapper (NEW)
**File**: `src/fraiseql/enterprise/rbac/rust_row_constraints.py`
**Purpose**: Python wrapper for Rust RowConstraintResolver similar to RustPermissionResolver

```python
class RustRowConstraintResolver:
    """Row constraint resolver using Rust implementation."""

    def __init__(self, pool: DatabasePool, cache_capacity: int = 10000):
        """Initialize from database pool."""
        self._rust_resolver = PyRowConstraintResolver(pool, cache_capacity)

    async def get_row_filters(
        self,
        user_id: UUID,
        table_name: str,
        roles: list[Role],
        tenant_id: Optional[UUID] = None,
    ) -> Optional[RowFilter]:
        """Get row-level filters for user on table."""
        # TODO: Implement async wrapper for Rust async method

    def invalidate_user(self, user_id: UUID) -> None:
        """Invalidate user cache on role changes."""

    def clear_cache(self) -> None:
        """Clear entire constraint cache."""
```

**Key Details**:
- Import `PyRowConstraintResolver` from `fraiseql._fraiseql_rs`
- Follow same pattern as `RustPermissionResolver`
- Handle `None` returns gracefully (no constraint = no filter)

### 2. Create `RustWhereMerger` Wrapper (NEW)
**File**: `src/fraiseql/enterprise/rbac/rust_where_merger.py`
**Purpose**: Python wrapper for Rust WhereMerger

```python
class RustWhereMerger:
    """WHERE clause merger using Rust implementation."""

    @staticmethod
    def merge_where(
        explicit_where: Optional[dict[str, Any]],
        row_filter: Optional[dict[str, Any]],
        strategy: str = "error",
    ) -> Optional[dict[str, Any]]:
        """Merge explicit WHERE with row-level filter."""
        # Convert to JSON, call Rust, convert back

    @staticmethod
    def validate_where(where_clause: dict[str, Any]) -> bool:
        """Validate WHERE clause structure."""
```

**Key Details**:
- Converts Python dicts ↔ JSON strings for Rust
- Handles 3 conflict strategies: "error", "override", "log"
- Returns merged WHERE dict or None
- Converts Rust errors to Python exceptions

### 3. Extend `RbacMiddleware` (MODIFY)
**File**: `src/fraiseql/enterprise/rbac/middleware.py`
**Changes**:
- Add row constraint resolver initialization
- Add method to resolve row filters for request
- Add row filter to context for use by resolvers

```python
class RbacMiddleware:
    def __init__(
        self,
        permission_resolver: Optional[PermissionResolver] = None,
        row_constraint_resolver: Optional[RustRowConstraintResolver] = None,
    ):
        self.permission_resolver = permission_resolver
        self.row_constraint_resolver = row_constraint_resolver

    async def _middleware(self, next_, root, info, **kwargs):
        # Existing permission resolution...

        # NEW: Add row-level filters to context
        if "row_filters" not in context:
            filters = await self._get_row_filters(context)
            context["row_filters"] = filters

    async def _get_row_filters(self, context) -> Optional[dict]:
        """Resolve row-level filters for request."""
        resolver = self.row_constraint_resolver
        if not resolver:
            return None

        user_id = context.get("user_id")
        table_name = context.get("table_name")  # From GraphQL query info
        roles = context.get("user_roles", [])
        tenant_id = context.get("tenant_id")

        if not all([user_id, table_name, roles]):
            return None

        # Get row filter from Rust resolver
        row_filter = await resolver.get_row_filters(
            user_id, table_name, roles, tenant_id
        )

        if not row_filter:
            return None

        # Convert to WHERE clause fragment
        return {
            row_filter.field: {"eq": row_filter.value}
        }
```

### 4. Integrate WHERE Merging into Query Resolution (MODIFY)
**File**: `src/fraiseql/gql/builders/query_builder.py`
**Changes**:
- Extract row filters from context during query execution
- Call WhereMerger to combine with explicit WHERE
- Pass merged WHERE to database layer

```python
def _build_resolver(self, query_func, element_type):
    """Build resolver with row-level filtering."""

    async def resolver(root, info, where=None, **kwargs):
        # Get row filters from middleware context
        row_filters = info.context.get("row_filters")

        # Merge WHERE clauses if needed
        if row_filters:
            merged = RustWhereMerger.merge_where(
                where, row_filters, strategy="error"
            )
        else:
            merged = where

        # Execute query with merged WHERE
        return await query_func(*args, where=merged, **kwargs)

    return resolver
```

### 5. Add Table Name to Context (MODIFY)
**File**: `src/fraiseql/enterprise/rbac/middleware.py`
**Purpose**: Extract table name from GraphQL query/field name for row filter lookup

```python
def _extract_table_name(self, info) -> Optional[str]:
    """Extract table name from GraphQL query field name."""
    # GraphQL field name → table name mapping
    # e.g., "documents" → "documents", "user_by_id" → "users"

    field_name = info.field_name

    # Try direct mapping first
    if self._table_exists(field_name):
        return field_name

    # Try singularize (documents → document)
    if self._table_exists(singularize(field_name)):
        return singularize(field_name)

    return None
```

## Implementation Steps

### Step 1: Create Rust Row Constraints Wrapper
- Create `src/fraiseql/enterprise/rbac/rust_row_constraints.py`
- Import `PyRowConstraintResolver` from `fraiseql._fraiseql_rs`
- Implement `RustRowConstraintResolver` class with:
  - `__init__(pool, cache_capacity)`
  - `async get_row_filters(user_id, table_name, roles, tenant_id)`
  - `invalidate_user(user_id)`
  - `clear_cache()`
- Add import error handling (graceful fallback if Rust not available)

### Step 2: Create Rust WHERE Merger Wrapper
- Create `src/fraiseql/enterprise/rbac/rust_where_merger.py`
- Import `PyWhereMerger` from `fraiseql._fraiseql_rs`
- Implement `RustWhereMerger` class with:
  - Static method `merge_where(explicit, row_filter, strategy)`
  - Static method `validate_where(where_clause)`
- Handle JSON conversion (Python dict ↔ JSON string)
- Convert Rust errors to Python exceptions

### Step 3: Extend RbacMiddleware
- Modify `src/fraiseql/enterprise/rbac/middleware.py`
- Add `row_constraint_resolver` parameter to `__init__`
- Add `_get_row_filters(context)` method
- Call row filter resolution in middleware
- Add row filters to GraphQL context
- Implement `_extract_table_name(info)` method

### Step 4: Integrate WHERE Merging into Query Resolution
- Modify `src/fraiseql/gql/builders/query_builder.py`
- Update resolver building to check for row filters in context
- Call `RustWhereMerger.merge_where()` when resolving queries
- Pass merged WHERE to database layer

### Step 5: Testing & Validation
- Create unit tests for both wrappers
- Create integration tests with real GraphQL queries
- Verify row filtering behavior
- Validate conflict detection

## Error Handling Strategy

### Conflict Handling (3 Strategies)
1. **"error"** (Default): Raise exception on conflict
   - Use for strict enforcement
   - Catches attempts to bypass auth filters

2. **"override"** (Auth-safe): Row filter takes precedence
   - User's explicit WHERE is ignored
   - Ensures auth filter always applies

3. **"log"** (Permissive): Log but continue
   - Both filters apply via AND composition
   - For complex multi-field scenarios

### Error Mapping
- `ConflictingFields` → `GraphQLError("Permission denied: conflicting WHERE conditions")`
- `InvalidStructure` → `GraphQLError("Invalid WHERE clause structure")`
- `SerializationError` → `GraphQLError("Internal error processing WHERE clause")`

## Performance Considerations

### Cache Integration
- Row constraint resolution uses same LRU + TTL strategy as permissions
- Cache key: `{user_id}:{table_name}:{tenant_id}`
- TTL: 5 minutes (configurable)
- Capacity: 10,000 entries (configurable)

### Expected Performance
- Cached lookup: <0.1ms
- Uncached (DB query): <1ms per table
- WHERE merging: <0.05ms

### Invalidation
- On role changes: `invalidate_user()` clears user's filters
- On row constraint changes: `clear_cache()` for tenant/all

## Files to Create/Modify

### New Files
1. `src/fraiseql/enterprise/rbac/rust_row_constraints.py` (150 LOC)
2. `src/fraiseql/enterprise/rbac/rust_where_merger.py` (200 LOC)

### Modified Files
1. `src/fraiseql/enterprise/rbac/middleware.py` (+100 LOC)
2. `src/fraiseql/gql/builders/query_builder.py` (+50 LOC)
3. `src/fraiseql/enterprise/rbac/__init__.py` (add exports)

### Total Impact
- New code: ~350 LOC
- Modified: ~150 LOC
- Test coverage: 20+ test cases

## Success Criteria

✅ **Functional**:
- Row filters are resolved from database
- WHERE clause merging prevents conflicts
- Merged WHERE applied to queries
- Row-level filtering works end-to-end

✅ **Performance**:
- Cache hits <0.1ms
- DB queries <1ms
- WHERE merge <0.05ms

✅ **Safety**:
- Auth filters cannot be bypassed
- Conflicts properly detected and handled
- Error messages are clear

✅ **Compatibility**:
- Existing middleware still works
- Optional row constraint resolver (graceful fallback)
- Backward compatible with current queries

## Next Steps (Phase 5)
1. Create database schema migration for `tb_row_constraint` table
2. Apply migration to development database
3. Seed test data with sample row constraints

## Next Steps (Phase 6)
1. Comprehensive unit tests for wrappers
2. Integration tests with GraphQL
3. Performance benchmarks
4. Documentation and examples
