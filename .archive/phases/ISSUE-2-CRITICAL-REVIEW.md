# Critical Plan Review: Row-Level Auth Middleware

**Date**: January 4, 2026
**Status**: PLAN REVISION NEEDED

---

## DISCOVERY: Existing Infrastructure

During critical review, identified that FraiseQL ALREADY HAS a solid RBAC foundation:

### ✅ Already Implemented

1. **RbacMiddleware** (`src/fraiseql/enterprise/rbac/middleware.py`)
   - Extracts user/tenant context from GraphQL requests
   - Injects PermissionResolver into GraphQL context
   - Manages request-level cache lifecycle
   - Logs authorization events

2. **PermissionResolver** (`src/fraiseql/enterprise/rbac/resolver.py`)
   - Computes effective permissions from role hierarchy
   - 2-layer caching: request-level + PostgreSQL
   - Automatic invalidation via domain versioning
   - Multi-tenant support
   - Performance: <0.5ms cached, <100ms uncached

3. **PermissionCache** (`src/fraiseql/enterprise/rbac/cache.py`)
   - PostgreSQL-native caching (0.1-0.3ms)
   - Domain versioning for automatic invalidation
   - CASCADE rules for hierarchical invalidation

4. **RoleHierarchy** (`src/fraiseql/enterprise/rbac/hierarchy.py`)
   - Hierarchical role inheritance
   - Transitive permission computation
   - Multi-tenant role scoping

### ❌ NOT Implemented (What We Actually Need)

The missing piece is **automatic WHERE clause injection based on permissions**. Currently:

```python
# What exists: Query-level auth (which fields can user see?)
@query
async def users(parent, info: Info) -> List[User]:
    # RbacMiddleware provides permission_resolver in context
    # Field directives can check: @directive(requires: "admin")
    # But... NO automatic row filtering!

    # Developer must manually do this:
    users = await repository.get_users(
        where={"tenant_id": info.context["user"].tenant_id}  # ← MANUAL
    )
    return users

# What we need: Automatic row filtering
# After this implementation, middleware should automatically inject:
where_clause = await resolver.get_row_filters(table="users", user=user, roles=roles)
# Result: {"tenant_id": {"eq": user.tenant_id}}
# Merged with explicit WHERE: {status: {eq: "active"}}
# Final: {AND: [{tenant_id: {eq: user.tenant_id}}, {status: {eq: "active"}}]}
```

---

## REVISED PLAN: Three New Components Needed

### 1. **RowFilterResolver** - Resolve table-level access constraints

**What it does**: Given a user + table + roles, determines what rows they can access

**Input**:
- `user_id`: UUID
- `table_name`: str (e.g., "users", "documents")
- `roles`: list[Role] (from existing PermissionResolver)
- `user_context`: dict (tenant_id, department, etc.)

**Output**:
```python
{
    "type": "ownership",
    "field": "owner_id",
    "value": user_id
}
# OR
{
    "type": "tenant",
    "field": "tenant_id",
    "value": tenant_id
}
# OR
{
    "type": "deny"  # User has no access to this table
}
```

**New file**: `src/fraiseql/enterprise/rbac/row_filter_resolver.py` (~300 LOC)

**Uses existing**:
- `PermissionResolver` - Get user permissions
- `PermissionCache` - Cache filter results
- Database schema - Query row constraint definitions

---

### 2. **RowWhereClauseBuilder** - Merge auth filters with explicit WHERE

**What it does**: Combine row-level auth filters with explicit GraphQL WHERE clauses

**Example**:
```
Explicit WHERE: {status: {eq: "active"}}
Auth filter:    {owner_id: {eq: user_id}}
Result:         {AND: [{status: {eq: "active"}}, {owner_id: {eq: user_id}}]}
```

**New file**: `src/fraiseql/enterprise/rbac/auth_where_builder.py` (~200 LOC)

**Uses existing**:
- `WhereClause` class - Where normalization
- `normalize_dict_where()` - Convert to standard format
- Rust WHERE pipeline - Execute merged clauses

---

### 3. **RowLevelAuthMiddleware** - Inject filters at query resolution time

**What it does**:
1. Intercepts GraphQL field resolution
2. Detects table being queried
3. Resolves row filters from RowFilterResolver
4. Merges with explicit WHERE using RowWhereClauseBuilder
5. Injects merged WHERE into query arguments

**New file**: `src/fraiseql/enterprise/rbac/row_level_middleware.py` (~250 LOC)

**Stacks with existing**:
- RbacMiddleware (layer below - provides context)
- Strawberry middleware stack (layer above - receives filtered queries)

---

## KEY INSIGHT: WHERE CLAUSE INTEGRATION POINT

The critical integration point is in the GraphQL field resolver. Currently:

```python
# In fraiseql/core/graphql_type.py (existing resolver code)
async def resolve_list_query(info, where=None, **kwargs):
    # 1. Get explicit WHERE from GraphQL args
    explicit_where = where or {}

    # 2. NEW: Get row-level filters from RowFilterResolver
    # 3. NEW: Merge using RowWhereClauseBuilder
    # 4. Normalize to WhereClause (existing code)
    # 5. Execute via Rust pipeline (existing code)
```

This is **non-invasive** because:
- Existing code path still works
- WHERE merging happens before normalization
- Rust pipeline sees standard WHERE clauses (no changes needed)
- Field directives still work (@directive checks)

---

## REVISED SCOPE

### Phase 1: RowFilterResolver (1-2 hours)
- Create filter resolution logic
- Query database for table row constraints
- Build WHERE clause fragments from constraints
- Add caching layer

### Phase 2: RowWhereClauseBuilder (1 hour)
- Implement WHERE clause merging
- Add conflict detection
- Validate merged clauses

### Phase 3: RowLevelAuthMiddleware (1-2 hours)
- Create middleware that calls above two components
- Integrate with RbacMiddleware stack
- Store filters in GraphQL context

### Phase 4: Integration Points (1 hour)
- Modify `graphql_type.py` resolver to use filters
- Ensure Rust pipeline receives merged WHERE
- Add integration test with real queries

### Phase 5: Testing (2 hours)
- Unit tests for each component
- Integration tests with GraphQL queries
- Security tests (bypass attempts)

**Total**: 6-8 hours (SAME AS PLANNED)

---

## DEPENDENCY CHAIN

```
RowFilterResolver
    ↓ (uses)
PermissionResolver (EXISTING)
PermissionCache (EXISTING)

RowWhereClauseBuilder
    ↓ (uses)
WhereClause (EXISTING)
normalize_dict_where() (EXISTING)

RowLevelAuthMiddleware
    ↓ (uses)
RowFilterResolver (NEW)
RowWhereClauseBuilder (NEW)
RbacMiddleware (EXISTING)

graphql_type.py resolver (MODIFY - 20 LOC)
    ↓ (uses)
RowLevelAuthMiddleware (NEW)
```

---

## WHAT WE CAN LEVERAGE (AVOID REIMPLEMENTING)

✅ **RbacMiddleware**: Already extracts user context, injects permission resolver
✅ **PermissionResolver**: Already computes effective permissions with caching
✅ **PermissionCache**: Already provides 2-layer caching with invalidation
✅ **WhereClause infrastructure**: Already handles WHERE normalization
✅ **Rust pipeline**: Already executes WHERE clauses efficiently

---

## WHAT WE MUST BUILD

🔨 **RowFilterResolver**: Translate permissions → WHERE clause conditions
🔨 **RowWhereClauseBuilder**: Merge explicit WHERE + auth filters
🔨 **RowLevelAuthMiddleware**: Orchestrate the above in middleware stack
🔨 **Configuration**: Define which tables need row filtering + filter rules

---

## CRITICAL SUCCESS FACTORS

1. **Leverage existing caching**: Use PermissionCache for filter results (<1ms cached)
2. **Minimal code changes**: Only modify graphql_type.py resolver (~20 LOC)
3. **Zero performance regression**: Merge overhead <0.5ms, Rust sees standard WHERE
4. **Backward compatible**: Existing manual WHERE clauses still work
5. **Auditable**: Log all injected filters for compliance

---

## REVISED IMPLEMENTATION APPROACH

**NOT** creating a new generic middleware framework.
**Instead** creating 3 focused components that:
1. Query database for table row constraints
2. Convert constraints to WHERE conditions
3. Inject into GraphQL context for use by resolvers

This is more pragmatic and reuses existing infrastructure.

---

## NEXT STEPS

1. ✅ Review this critical assessment
2. ⏳ Approve revised scope (3 components instead of 5 complex modules)
3. ⏳ Begin Phase 1: RowFilterResolver
4. ⏳ Proceed with Phases 2-5 as planned

---

**Key Takeaway**: We don't need to build a new RBAC system (already exists). We need to add the missing piece: automatic translation of permissions to WHERE clause filters at query resolution time.
