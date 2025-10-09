# Nested Object Resolution in FraiseQL

This guide explains how FraiseQL handles nested objects that have their own `sql_source`, and how to control whether they should be resolved via separate queries or use embedded data from the parent's JSONB column.

## Table of Contents

- [Overview](#overview)
- [Default Behavior: Embedded Data](#default-behavior-embedded-data)
- [Explicit Nested Resolution](#explicit-nested-resolution)
- [When to Use Each Approach](#when-to-use-each-approach)
- [Common Patterns](#common-patterns)
- [Performance Considerations](#performance-considerations)
- [Troubleshooting](#troubleshooting)

## Overview

When a GraphQL type with `sql_source` appears as a field in another type, FraiseQL needs to know whether to:

1. **Use embedded data** from the parent's JSONB column (default)
2. **Make a separate query** to the nested type's sql_source table

This behavior is controlled by the `resolve_nested` parameter in the `@type` decorator.

## Default Behavior: Embedded Data

By default (`resolve_nested=False`), FraiseQL assumes nested objects are embedded in the parent's JSONB data column. This is the most common pattern when using PostgreSQL views that pre-join related data.

### Example: Embedded Organization in User

```python
from fraiseql import type
from uuid import UUID
from typing import Optional

# Organization type - data will be embedded when nested
@type(sql_source="v_organizations")
class Organization:
    id: UUID
    name: str
    identifier: str
    status: str

# User type with embedded organization
@type(sql_source="v_users")
class User:
    id: UUID
    first_name: str
    last_name: str
    email: str
    organization: Optional[Organization] = None  # Uses embedded data
```

### Database View Structure

The view should include the nested object's data in the JSONB column:

```sql
CREATE VIEW v_users AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'first_name', u.first_name,
        'last_name', u.last_name,
        'email', u.email,
        'organization', jsonb_build_object(  -- Embedded organization
            'id', o.id,
            'name', o.name,
            'identifier', o.identifier,
            'status', o.status
        )
    ) AS data
FROM users u
LEFT JOIN organizations o ON u.organization_id = o.id;
```

### GraphQL Query

```graphql
query GetUser {
  user(id: "123") {
    firstName
    lastName
    organization {  # No separate query - uses embedded data
      name
      identifier
    }
  }
}
```

### Benefits

- ✅ **No N+1 queries** - All data fetched in one query
- ✅ **No tenant_id required** for nested objects
- ✅ **Better performance** - Single database roundtrip
- ✅ **Simpler context** - No need to pass additional parameters

## Explicit Nested Resolution

When you need FraiseQL to make separate queries for nested objects (true relational data), set `resolve_nested=True`.

### Example: Department as a Relation

```python
# Department will be queried separately when nested
@type(sql_source="v_departments", resolve_nested=True)
class Department:
    id: UUID
    name: str
    code: str

# Employee with department as a foreign key relation
@type(sql_source="v_employees")
class Employee:
    id: UUID
    name: str
    department_id: Optional[UUID] = None  # Foreign key
    department: Optional[Department] = None  # Will query v_departments
```

### Database View Structure

The parent view only includes the foreign key, not the full nested data:

```sql
CREATE VIEW v_employees AS
SELECT
    e.id,
    jsonb_build_object(
        'id', e.id,
        'name', e.name,
        'department_id', e.department_id  -- Only the FK, not full data
    ) AS data
FROM employees e;

-- Separate view for departments
CREATE VIEW v_departments AS
SELECT
    d.id,
    d.tenant_id,  -- May require tenant_id for multi-tenant apps
    jsonb_build_object(
        'id', d.id,
        'name', d.name,
        'code', d.code
    ) AS data
FROM departments d;
```

### GraphQL Query

```graphql
query GetEmployee {
  employee(id: "456") {
    name
    department {  # Triggers separate query to v_departments
      name
      code
    }
  }
}
```

### Requirements

- Context must include necessary parameters (e.g., `tenant_id`)
- Parent must have the foreign key field (e.g., `department_id`)
- Nested type's sql_source must be queryable with available context

## When to Use Each Approach

### Use Default (Embedded Data) When:

- ✅ Your views pre-join and embed related data
- ✅ You want optimal performance (single query)
- ✅ The nested data is relatively small
- ✅ You're using JSONB columns for denormalization
- ✅ You want to avoid N+1 query problems

### Use `resolve_nested=True` When:

- ✅ Nested data is truly relational (not embedded)
- ✅ You need fresh data from the source table
- ✅ The nested data is large and rarely accessed
- ✅ You have complex authorization rules per table
- ✅ You're migrating from a traditional ORM pattern

## Common Patterns

### Pattern 1: Multi-Tenant Application with Embedded Data

```python
@type(sql_source="v_tenants")
class Tenant:
    id: UUID
    name: str
    plan: str

@type(sql_source="v_users_with_tenant")
class User:
    id: UUID
    email: str
    tenant: Tenant  # Embedded - no tenant_id issues
```

### Pattern 2: Large Related Data (Use Separate Queries)

```python
@type(sql_source="v_documents", resolve_nested=True)
class Document:
    id: UUID
    title: str
    content: str  # Large text field
    size_bytes: int

@type(sql_source="v_projects")
class Project:
    id: UUID
    name: str
    document_ids: list[UUID]
    documents: list[Document]  # Resolved separately to avoid bloat
```

### Pattern 3: Mixed Approach

```python
# Frequently accessed, small data - embedded by default
@type(sql_source="v_categories")
class Category:
    id: UUID
    name: str
    color: str

# Large, rarely accessed data - resolved separately
@type(sql_source="v_images", resolve_nested=True)
class Image:
    id: UUID
    url: str
    data: bytes  # Large binary data

@type(sql_source="v_products")
class Product:
    id: UUID
    name: str
    category: Category  # Embedded (small, frequent)
    images: list[Image]  # Separate queries (large, infrequent)
```

## Performance Considerations

### Embedded Data (Default)
```
Query: { user { organization { name } } }

Execution:

1. SELECT data FROM v_users WHERE id = ?
   ↓
   Returns: { user: { organization: { name: "Acme Corp" } } }

Total queries: 1
```

### Nested Resolution (`resolve_nested=True`)
```
Query: { employees { department { name } } }  # N employees

Execution:

1. SELECT data FROM v_employees
   ↓

2. SELECT data FROM v_departments WHERE id = ? (for each unique dept)
   ↓
   Returns merged data

Total queries: 1 + number of unique departments
```

## Troubleshooting

### Error: "missing a required argument: 'tenant_id'"

**Cause**: A nested type with `sql_source` is trying to resolve separately but lacks required context.

**Solutions**:

1. Remove `resolve_nested=True` if data should be embedded
2. Ensure the view includes embedded data in JSONB
3. If separate resolution is needed, provide `tenant_id` in context

### Error: "Cannot read property 'name' of null"

**Cause**: Expected embedded data is missing from parent's JSONB.

**Solutions**:

1. Update view to include nested object in JSONB
2. Use LEFT JOIN to handle optional relationships
3. Set field as `Optional[Type]` in Python

### N+1 Query Problems

**Symptom**: Many queries executed for a list with nested objects.

**Solution**:

- Remove `resolve_nested=True` unless absolutely necessary
- Ensure views embed frequently accessed nested data
- Consider using DataLoader pattern for `resolve_nested=True` cases

## Migration Guide

### From Traditional ORM to FraiseQL

If migrating from an ORM that always does separate queries:

```python
# Step 1: Start with resolve_nested=True (matches ORM behavior)
@type(sql_source="departments", resolve_nested=True)
class Department:
    ...

# Step 2: Create views with embedded data
CREATE VIEW v_employees_with_dept AS ...

# Step 3: Remove resolve_nested=True after views are ready
@type(sql_source="departments")  # Now uses embedded data
class Department:
    ...
```

## Best Practices

1. **Default to embedded data** - Better performance, simpler code
2. **Document your choice** - Add comments explaining why `resolve_nested=True` is used
3. **Monitor query performance** - Use query logging to detect N+1 problems
4. **Design views thoughtfully** - Include commonly accessed nested data
5. **Use resolve_nested sparingly** - Only when truly necessary

## Summary

- **Default behavior** (`resolve_nested=False`): Uses embedded JSONB data
- **Explicit resolution** (`resolve_nested=True`): Makes separate queries
- **Choose based on**: Data size, access patterns, and performance needs
- **Best practice**: Use embedded data unless you have a specific reason not to
