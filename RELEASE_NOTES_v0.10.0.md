# Release Notes - FraiseQL v0.10.0

## ✨ Context Parameters Support for Turbo Queries

### Release Date: 2025-10-04
### Type: Feature Enhancement

## Summary

This release adds `context_params` support to TurboQuery, enabling multi-tenant turbo-optimized queries with row-level security. Turbo queries can now access authentication context (tenant_id, user_id) from JWT, just like mutations do.

## 🚨 Problem Solved

Before v0.10.0, turbo queries could not access context parameters, forcing multi-tenant applications to choose between:
- **Option A**: Use turbo router for 10x+ performance, but lose tenant isolation ❌
- **Option B**: Use normal queries with tenant_id, but lose turbo performance ❌

Neither option was acceptable for production multi-tenant SaaS applications.

### Before (Broken) ❌
```python
# Turbo query WITHOUT context support
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_allocations(%(period)s)::json",
    param_mapping={"period": "period"},  # Only variables, no context!
)

# GraphQL Request with JWT → Context {tenant_id: "tenant-123"}
# ❌ SQL receives: fn_get_allocations('CURRENT')
# ❌ Missing tenant_id → Returns data from ALL tenants!
# ❌ CRITICAL SECURITY ISSUE
```

### After (Fixed) ✅
```python
# Turbo query WITH context support
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_allocations(%(period)s, %(tenant_id)s)::json",
    param_mapping={"period": "period"},         # From GraphQL variables
    context_params={"tenant_id": "tenant_id"},  # ✨ NEW: From JWT context
)

# GraphQL Request with JWT → Context {tenant_id: "tenant-123"}
# ✅ SQL receives: fn_get_allocations('CURRENT', 'tenant-123')
# ✅ Tenant isolation enforced
# ✅ 10x+ turbo performance maintained
```

## Impact

### Who Benefits?
- **Multi-tenant SaaS applications** - Can now use turbo router with tenant isolation
- **Enterprise applications** - Row-level security with high performance
- **Audit-compliant systems** - Track user_id for all data access
- **High-traffic APIs** - Combine caching with security

### Severity: Major Feature
- **Security**: Enables tenant isolation for turbo queries
- **Performance**: No longer forced to choose between speed and security
- **API Consistency**: Turbo queries now match mutation pattern
- **Production Ready**: Multi-tenant turbo queries are now safe

## Technical Details

### Implementation
This feature mirrors the exact pattern used by mutations:

1. **TurboQuery Dataclass**: Added `context_params: dict[str, str] | None` field
2. **TurboRouter.execute()**: Maps context values to SQL parameters
3. **Error Handling**: Validates required context parameters are present
4. **Backward Compatible**: context_params is optional (None by default)

### Code Changes
```python
# src/fraiseql/fastapi/turbo.py

@dataclass
class TurboQuery:
    graphql_query: str
    sql_template: str
    param_mapping: dict[str, str]
    operation_name: str | None = None
    apollo_client_hash: str | None = None
    context_params: dict[str, str] | None = None  # ✨ NEW

# TurboRouter.execute() now maps context parameters:
if turbo_query.context_params:
    for context_key, sql_param in turbo_query.context_params.items():
        context_value = context.get(context_key)
        if context_value is None:
            raise ValueError(f"Required context parameter '{context_key}' not found")
        sql_params[sql_param] = context_value
```

### Performance Improvements
- **No performance penalty** - context mapping is O(1) dictionary lookup
- **Maintains 10x+ turbo speedup** vs normal GraphQL queries
- **Cache-friendly** - tenant_id can be included in cache keys

## Migration Guide

### No Breaking Changes ✅
This feature is 100% backward compatible:

1. **Existing turbo queries** continue to work without modification
2. **context_params is optional** - defaults to None
3. **No schema changes** required

### Recommended Migration Path

#### Step 1: Update FraiseQL
```bash
pip install fraiseql==0.10.0
```

#### Step 2: Update Turbo Query Registration
```python
# Before: Turbo query without context
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_data(%(filter)s)::json",
    param_mapping={"filter": "filter"},
)

# After: Turbo query with context
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_data(%(filter)s, %(tenant_id)s, %(user_id)s)::json",
    param_mapping={"filter": "filter"},
    context_params={                           # ✨ NEW
        "tenant_id": "tenant_id",
        "user_id": "user_id"
    }
)
```

#### Step 3: Update SQL Functions
```sql
-- Before: Function without context
CREATE OR REPLACE FUNCTION turbo.fn_get_data(
    p_filter text
) RETURNS json AS $$
BEGIN
    -- ❌ No tenant isolation
    RETURN (SELECT json_agg(row_to_json(t)) FROM data t WHERE status = p_filter);
END;
$$ LANGUAGE plpgsql;

-- After: Function with context parameters
CREATE OR REPLACE FUNCTION turbo.fn_get_data(
    p_filter text,
    p_tenant_id uuid,    -- ✨ NEW: From JWT context
    p_user_id uuid       -- ✨ NEW: From JWT context
) RETURNS json AS $$
BEGIN
    -- ✅ Tenant isolation enforced
    RETURN (
        SELECT json_agg(row_to_json(t))
        FROM data t
        WHERE status = p_filter
          AND tenant_id = p_tenant_id  -- ✨ Row-level security
    );
END;
$$ LANGUAGE plpgsql;
```

## Usage Examples

### Example 1: Multi-Tenant Data Access
```python
# Register turbo query with tenant isolation
turbo_query = TurboQuery(
    graphql_query="""
        query GetAllocations($period: String!) {
            allocations(period: $period) {
                id
                name
                amount
            }
        }
    """,
    sql_template="SELECT turbo.fn_get_allocations(%(period)s, %(tenant_id)s)::json",
    param_mapping={"period": "period"},
    operation_name="GetAllocations",
    context_params={"tenant_id": "tenant_id"},  # ✨ From JWT
)

registry.register(turbo_query)

# Execute with JWT context
result = await turbo_router.execute(
    query=query,
    variables={"period": "CURRENT"},
    context={
        "db": db,
        "tenant_id": "tenant-123",  # From JWT authentication
        "user_id": "user-456"
    }
)
# ✅ Returns ONLY tenant-123's allocations
```

### Example 2: Audit Logging
```python
# Track which user accessed data
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_sensitive_data(%(tenant_id)s, %(user_id)s)::json",
    param_mapping={},
    context_params={
        "tenant_id": "tenant_id",
        "user_id": "user_id"  # ✨ Audit trail
    }
)

# SQL function logs access
CREATE FUNCTION turbo.fn_get_sensitive_data(
    p_tenant_id uuid,
    p_user_id uuid
) RETURNS json AS $$
BEGIN
    -- Log access for compliance
    INSERT INTO audit_log (tenant_id, user_id, action, timestamp)
    VALUES (p_tenant_id, p_user_id, 'VIEW_SENSITIVE_DATA', NOW());

    RETURN (SELECT json_agg(...) FROM sensitive_data WHERE tenant_id = p_tenant_id);
END;
$$ LANGUAGE plpgsql;
```

### Example 3: Row-Level Security
```python
# Combine with PostgreSQL RLS policies
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="""
        SELECT turbo.fn_get_data_with_rls(
            %(filters)s::jsonb,
            %(tenant_id)s,
            %(user_role)s
        )::json
    """,
    param_mapping={"filters": "filters"},
    context_params={
        "tenant_id": "tenant_id",
        "user_role": "role"  # ✨ Role-based access
    }
)
```

## Error Handling

### Missing Required Context Parameter
```python
# Query registered with context_params
turbo_query = TurboQuery(
    # ...
    context_params={"tenant_id": "tenant_id"}
)

# Execute without tenant_id in context
await turbo_router.execute(
    query=query,
    variables={...},
    context={"db": db}  # ❌ Missing tenant_id
)

# Raises: ValueError("Required context parameter 'tenant_id' not found in GraphQL context for turbo query")
```

## Testing

### New Tests Added
- `test_turbo_query_with_context_params` - Verifies context params mapped to SQL
- `test_turbo_query_missing_required_context_param` - Validates error handling

All 3,305 existing tests pass with 100% backward compatibility.

## Upgrading

```bash
pip install fraiseql==0.10.0
```

## Benefits Summary

✅ **Multi-tenant support** for turbo queries with row-level security
✅ **10x+ performance** maintained with tenant isolation
✅ **Security** - tenant_id from server-side JWT, not client input
✅ **Consistent API** - matches mutation `context_params` pattern
✅ **Audit trails** - track user_id for created_by/updated_by
✅ **Cache isolation** - include tenant_id in cache keys
✅ **Production ready** - enterprise SaaS applications can now use turbo router

## Related Links

- Feature Implementation: [Pull Request](https://github.com/fraiseql/fraiseql/pull/XX)
- Original Issue: `/tmp/fraiseql_turbo_context_params_issue.md`
- Test Coverage: `tests/integration/caching/test_turbo_router.py`

## Acknowledgments

Thank you to the PrintOptim team for the detailed analysis that identified this critical gap in turbo router functionality for multi-tenant applications.

---

**Note:** If you're building a multi-tenant SaaS application, upgrading to v0.10.0 enables you to use the turbo router with full tenant isolation and row-level security.
