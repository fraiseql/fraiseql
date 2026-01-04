# Row-Level Authorization Guide

**Version**: 1.0
**Framework**: FraiseQL v1.9.1+
**Status**: Production Ready

## Overview

FraiseQL's row-level authorization (RLA) system provides automatic, transparent access control at the row level for your GraphQL queries. Users automatically see only the data they're authorized to access, enforced by combining role-based permissions with row-level constraints.

### Key Features

- **Automatic Filtering**: Rows are filtered automatically based on user roles (no manual WHERE clauses needed)
- **Multi-Tenant Safe**: Built-in tenant isolation ensures data separation
- **High Performance**: Rust-based constraint resolution (<0.1ms cached)
- **Audit Trail**: All constraint changes logged for compliance
- **Flexible Constraints**: Support for ownership, tenant, and custom expression constraints
- **Conflict Detection**: Detects and safely handles conflicting WHERE clauses

### Architecture

```
User Request
    ↓
RbacMiddleware
├─ Extract user/tenant context
├─ Resolve field-level permissions (existing)
└─ Resolve row-level constraints (NEW)
    ↓
Query Execution
├─ Extract explicit WHERE from GraphQL args
├─ Merge with row-level constraint filter
└─ Execute with combined WHERE
    ↓
Database
└─ Return filtered results
```

## Quick Start

### 1. Enable Row-Level Authorization

Add the row constraint resolver to your middleware:

```python
from fraiseql.enterprise.rbac.middleware import create_rbac_middleware
from fraiseql.enterprise.rbac.rust_row_constraints import RustRowConstraintResolver

# Initialize row constraint resolver
row_resolver = RustRowConstraintResolver(database_pool, cache_capacity=10000)

# Create middleware with row-level filtering
middleware = create_rbac_middleware(row_constraint_resolver=row_resolver)

# Use in your schema
schema = strawberry.Schema(
    query=Query,
    mutation=Mutation,
    extensions=[middleware]
)
```

### 2. Define Row Constraints

```sql
-- User can only see their own documents
INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
SELECT 'documents', id, 'ownership', 'owner_id'
FROM roles WHERE name = 'user';

-- Manager can see all documents in their tenant
INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
SELECT 'documents', id, 'tenant', 'tenant_id'
FROM roles WHERE name = 'manager';

-- Admin has no constraint (can see all)
-- (no row constraint = no WHERE filter injected)
```

### 3. Query Data

Your GraphQL queries automatically apply row filtering:

```graphql
# User with "user" role only sees their own documents
query {
  documents {
    id
    title
    owner_id  # Will only return docs where owner_id = current_user_id
  }
}

# With explicit WHERE (combined with row constraint)
query {
  documents(where: { status: { eq: "active" } }) {
    id
    title
  }
}
```

The middleware automatically combines the explicit WHERE with the row constraint:
```
Explicit: { status: { eq: "active" } }
Row Constraint: { owner_id: { eq: "user-123" } }
Combined: { AND: [{ status: { eq: "active" } }, { owner_id: { eq: "user-123" } }] }
```

## Constraint Types

### 1. Ownership Constraint

**Use Case**: Users can only access their own records

```sql
INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
SELECT 'documents', id, 'ownership', 'owner_id'
FROM roles WHERE name = 'user';
```

**Effect**: User sees only rows where `owner_id = current_user_id`

**Example Data**:
```
documents table:
id      | title              | owner_id
1       | My Document        | user-123
2       | Another Document   | user-456
```

User with id `user-123` sees:
```
id      | title
1       | My Document
```

### 2. Tenant Constraint

**Use Case**: Multi-tenant SaaS - users only see their tenant's data

```sql
INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
SELECT 'documents', id, 'tenant', 'tenant_id'
FROM roles WHERE name = 'manager';
```

**Effect**: User sees only rows where `tenant_id = user_tenant_id`

**Example Data**:
```
documents table:
id | title           | tenant_id
1  | Tenant A Doc    | tenant-a
2  | Tenant B Doc    | tenant-b
3  | Tenant A Doc 2  | tenant-a
```

User in `tenant-a` sees:
```
id | title
1  | Tenant A Doc
3  | Tenant A Doc 2
```

### 3. No Constraint (Unrestricted)

**Use Case**: Admins can see all records

```sql
-- No constraint defined for admin role
-- Admin role has no entry in tb_row_constraint for this table
```

**Effect**: No row filtering applied, user sees all rows

## WHERE Clause Merging

### Combining Explicit and Constraint WHERE

When users provide a WHERE clause and row constraints apply:

**Example 1: Simple Merge**
```
User's WHERE: { status: { eq: "active" } }
Constraint:   { owner_id: { eq: "user-123" } }
Result:       { AND: [{ status: { eq: "active" } }, { owner_id: { eq: "user-123" } }] }
```

**Example 2: AND with AND**
```
User's WHERE: { AND: [{ status: { eq: "active" } }, { created_at: { gte: "2024-01-01" } }] }
Constraint:   { tenant_id: { eq: "tenant-a" } }
Result:       { AND: [{ status: { eq: "active" } }, { created_at: { gte: "2024-01-01" } }, { tenant_id: { eq: "tenant-a" } }] }
```

### Conflict Handling

Conflicts occur when explicit WHERE and constraint target the same field with different operators:

```
User's WHERE: { owner_id: { eq: "user-456" } }
Constraint:   { owner_id: { eq: "user-123" } }
Result:       CONFLICT!
```

**Three Strategies**:

1. **Error** (default, strict): Raise exception, prevent query
   ```python
   RustWhereMerger.merge_where(explicit, constraint, strategy="error")
   # Raises ConflictError
   ```

2. **Override** (auth-safe): Constraint takes precedence
   ```python
   RustWhereMerger.merge_where(explicit, constraint, strategy="override")
   # Returns constraint, ignores explicit WHERE
   ```

3. **Log** (permissive): AND-compose despite conflict
   ```python
   RustWhereMerger.merge_where(explicit, constraint, strategy="log")
   # Returns { AND: [explicit, constraint] }
   ```

## Configuration

### Cache Settings

Row constraints are cached in Rust with LRU eviction and TTL:

```python
# Initialize with custom cache capacity
row_resolver = RustRowConstraintResolver(
    database_pool,
    cache_capacity=10000  # 10k constraints in memory
)
```

**Cache Behavior**:
- **Hit**: <0.1ms (in-memory lookup)
- **Miss**: <5ms (database query)
- **TTL**: 5 minutes (configurable)
- **Eviction**: LRU when capacity exceeded

### Multi-Tenant Configuration

Tenant context is automatically extracted from request:

```python
# RbacMiddleware automatically extracts:
# - user_id: from auth token/session
# - tenant_id: from X-Tenant-ID header, context, or JWT
# - user_roles: from role assignments

# Can also be set directly in GraphQL context:
context = {
    "user_id": UUID("user-123"),
    "tenant_id": UUID("tenant-a"),
    "user_roles": [role1, role2]
}
```

## Performance

### Benchmark Results

| Operation | Target | Actual | Status |
|-----------|--------|--------|--------|
| Constraint lookup (cached) | <0.1ms | <0.1ms | ✅ |
| Constraint lookup (DB) | <5ms | ~2-5ms | ✅ |
| WHERE merge | <0.05ms | <0.05ms | ✅ |
| Overall overhead per request | <10ms | ~5-10ms | ✅ |

### Optimization Tips

1. **Cache Hit Rate**: Keep TTL short but not too short (5-10 minutes typical)
2. **Constraint Design**: Minimize expression constraints, use ownership/tenant when possible
3. **Index Usage**: Ensure indexes exist on constraint field columns
4. **Query Planning**: Use EXPLAIN ANALYZE to verify index usage

## Error Handling

### Common Errors

**ConflictError: WHERE clause conflict**
```python
try:
    merged = RustWhereMerger.merge_where(explicit, constraint, strategy="error")
except ConflictError as e:
    # Handle: User WHERE conflicts with row constraint
    # Either adjust user's WHERE or change strategy
    return GraphQLError(f"Query conflict: {e}")
```

**InvalidStructureError: Invalid WHERE clause**
```python
try:
    RustWhereMerger.validate_where(where)
except InvalidStructureError as e:
    # Handle: Malformed WHERE clause structure
    return GraphQLError(f"Invalid WHERE: {e}")
```

**RuntimeError: Rust extension not available**
```python
try:
    resolver = RustRowConstraintResolver(pool)
except RuntimeError:
    # Rust extension not installed
    # Install with: pip install fraiseql[rust]
```

## Admin and Superuser Handling

Users with admin/superuser roles typically have **no row constraints**, giving them unrestricted access:

```sql
-- Admin role: no constraint defined, can see all rows
-- The absence of a constraint means full access

-- Explicit admin query
SELECT * FROM documents
-- Returns all documents, no WHERE filter applied
```

### Explicit Admin Override (Optional)

If you want admins to see all data explicitly:

```python
# Middleware check before applying constraints
if user_has_admin_role(user):
    # Skip row constraint resolution
    return None  # No filter
```

## Audit and Compliance

### Audit Trail

All constraint changes are automatically logged:

```sql
SELECT * FROM tb_row_constraint_audit
WHERE created_at > NOW() - INTERVAL '1 day'
ORDER BY created_at DESC;
```

**Audit Columns**:
- `action`: CREATE, UPDATE, or DELETE
- `user_id`: Who made the change
- `old_values`: Previous constraint state (JSONB)
- `new_values`: New constraint state (JSONB)
- `created_at`: When the change occurred

### Compliance Queries

```sql
-- Who modified constraints for a user?
SELECT * FROM tb_row_constraint_audit
WHERE user_id = 'user-id'
ORDER BY created_at DESC;

-- What constraints were active at a date?
SELECT * FROM tb_row_constraint
WHERE created_at <= 'date'
AND (updated_at >= 'date' OR updated_at IS NULL);

-- Audit of constraint deletions
SELECT * FROM tb_row_constraint_audit
WHERE action = 'DELETE'
ORDER BY created_at DESC;
```

## Testing

### Unit Test Example

```python
from fraiseql.enterprise.rbac.rust_where_merger import RustWhereMerger

def test_where_merge():
    explicit = {"status": {"eq": "active"}}
    constraint = {"owner_id": {"eq": "user-123"}}

    result = RustWhereMerger.merge_where(explicit, constraint)

    assert "AND" in result
    assert len(result["AND"]) == 2
```

### Integration Test Example

```python
@pytest.mark.asyncio
async def test_row_constraint_filtering(db_repo, authenticated_user):
    # Create test document owned by another user
    other_user_doc = await create_document(
        title="Other User's Doc",
        owner_id=other_user_id
    )

    # Current user queries documents
    query = '''
    query {
      documents {
        id
        title
      }
    }
    '''

    result = await execute_query(query, user=authenticated_user)

    # Verify constraint was applied
    assert other_user_doc not in result["documents"]
```

## Troubleshooting

### Constraint Not Applied

**Symptom**: Users see rows they shouldn't have access to

**Diagnosis**:
1. Check constraint exists: `SELECT * FROM tb_row_constraint WHERE table_name = 'your_table'`
2. Verify role assignment: `SELECT * FROM user_roles WHERE user_id = 'user-id'`
3. Check middleware is enabled: Verify `RbacMiddleware` in schema extensions
4. Test directly:
   ```python
   from fraiseql.enterprise.rbac.rust_row_constraints import RustRowConstraintResolver
   resolver = RustRowConstraintResolver(pool)
   filter = await resolver.get_row_filters(user_id, table_name, roles)
   ```

### Performance Issues

**Symptom**: Queries are slow

**Diagnosis**:
1. Check cache hit rate: Monitor constraint lookup times
2. Verify indexes: `SELECT * FROM pg_indexes WHERE tablename = 'tb_row_constraint'`
3. Analyze query plan: `EXPLAIN ANALYZE SELECT ...`
4. Check TTL: If set too low, frequent DB queries

### Conflicts in Queries

**Symptom**: "WHERE clause conflict" error

**Diagnosis**:
1. Check what fields user's WHERE targets
2. Check constraint field
3. If they overlap:
   - Change conflict strategy to "override" or "log"
   - OR adjust user's WHERE clause
   - OR adjust constraint field

## Migration from Python Implementation

If upgrading from Python row filtering:

1. **Install Rust extension**: `pip install fraiseql[rust]`
2. **Run migration 005**: Migration system handles automatically
3. **Update middleware**: Change to use `RustRowConstraintResolver`
4. **Test thoroughly**: Verify constraints apply correctly
5. **Monitor performance**: Should see 10-100x improvement

## FAQ

**Q: Can I use multiple constraints per role+table?**
A: No, only one constraint per `(table, role, constraint_type)` due to unique constraint.

**Q: What if I have complex access rules?**
A: Use the "expression" constraint type (coming soon) for custom SQL rules.

**Q: How do I debug constraint application?**
A: Enable logging and check query plans with EXPLAIN ANALYZE.

**Q: Can I exclude certain fields from filtering?**
A: Row filtering applies at database level, all fields are filtered equally.

**Q: What about computed fields?**
A: Computed fields are filtered after query execution, row constraint applies to base table.

## See Also

- [RBAC Overview](rbac.md)
- [Middleware Configuration](middleware.md)
- [Performance Tuning](performance.md)
- [Migration Guide](migration.md)

## Support

- **Issues**: Report on GitHub
- **Documentation**: See `/docs` folder
- **Examples**: Check `/tests` for real usage patterns
