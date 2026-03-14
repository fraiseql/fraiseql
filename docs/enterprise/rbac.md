<!-- Skip to main content -->
---

title: Enterprise RBAC Documentation
description: FraiseQL's Enterprise Role-Based Access Control (RBAC) system provides:
keywords: []
tags: ["documentation", "reference"]
---

# Enterprise RBAC Documentation

**Status:** ✅ Production Ready
**Version:** FraiseQL v2.0.0-alpha.1+
**Topic**: Role-Based Access Control
**Performance**: <0.5ms cached, <100ms uncached

---

## Overview

FraiseQL's Enterprise Role-Based Access Control (RBAC) system provides:

- **Hierarchical Role Inheritance**: Up to 10 levels of role nesting
- **Two-Layer Permission Caching**: Request-level + PostgreSQL UNLOGGED table
- **Automatic Cache Invalidation**: Domain versioning prevents manual cache management
- **Field-Level Authorization**: GraphQL directive-based field permissions
- **Row-Level Security**: Automatic WHERE clause filtering based on roles
- **Multi-Tenant Support**: Tenant-scoped roles and permissions
- **Production-Grade Performance**: <0.5ms permission lookups (cached)

---

## Quick Start

### Basic Setup

```python
<!-- Code example in Python -->
from FraiseQL.enterprise.rbac import setup_rbac_cache
from FraiseQL.enterprise.rbac import PermissionResolver

# At application startup
async def app_startup(db_pool):
    # Initialize RBAC domain versioning and cascade rules
    await setup_rbac_cache(db_pool)

    # Create permission resolver
    resolver = PermissionResolver(db_pool)
```text
<!-- Code example in TEXT -->

### Using in GraphQL

```python
<!-- Code example in Python -->
from FraiseQL.enterprise.rbac.middleware import create_rbac_middleware

# Add RBAC middleware to GraphQL schema
schema = strawberry.Schema(
    query=Query,
    mutation=Mutation,
    extensions=[create_rbac_middleware(permission_resolver=resolver)]
)
```text
<!-- Code example in TEXT -->

### Field-Level Authorization

```python
<!-- Code example in Python -->
import strawberry
from FraiseQL.enterprise.rbac.directives import requires_permission, requires_role

@strawberry.type
class User:
    id: strawberry.ID
    name: str

    @requires_permission("user", "read_email")
    def email(self) -> str:
        """Only users with user:read_email permission can access"""
        return self.email_value

    @requires_role("admin")
    def salary(self) -> float:
        """Only admins can see salary"""
        return self.salary_value
```text
<!-- Code example in TEXT -->

---

## Architecture

### Role Hierarchy

FraiseQL supports hierarchical role inheritance where roles can inherit from parent roles.

```text
<!-- Code example in TEXT -->
┌─────────────┐
│   System    │  (root role)
└──────┬──────┘
       │
   ┌───┴───┐
   │       │
┌──▼──┐ ┌──▼──┐
│Admin│ │User │
└──┬──┘ └─────┘
   │
┌──▼─────┐
│Manager  │
└─────────┘
```text
<!-- Code example in TEXT -->

**Key Concepts**:

- Each role can have an optional `parent_role_id`
- Child roles inherit all permissions from parents
- Cycle detection prevents infinite loops (via PostgreSQL CTEs)
- Depth limit: 10 levels maximum

### Two-Layer Permission Cache

**Layer 1: Request-Level Cache**

- In-memory dictionary, scoped to single request
- Instant access: < 1 microsecond
- Automatic cleanup at request end
- Zero latency for repeated permission checks

**Layer 2: PostgreSQL UNLOGGED Table**

- Persistent cache across requests
- Domain versioning tracks cache validity
- Automatic cascade invalidation on changes
- Lookup performance: 0.1-0.3ms

**Cache Invalidation**:

```text
<!-- Code example in TEXT -->
User modifies permission
       ↓
domain_version incremented
       ↓
All cached permissions with old version invalidated
       ↓
Next request checks version, refreshes if needed
```text
<!-- Code example in TEXT -->

### Domain Versioning

FraiseQL uses domain versioning for automatic cache invalidation:

```sql
<!-- Code example in SQL -->
-- Each domain has a version
SELECT version FROM domain_versions WHERE domain = 'role';

-- When a role changes:
UPDATE domain_versions
SET version = version + 1
WHERE domain = 'role';

-- On permission lookup:
SELECT * FROM permission_cache
WHERE user_id = ?
  AND version = (SELECT version FROM domain_versions WHERE domain = 'role');
```text
<!-- Code example in TEXT -->

---

## Core Concepts

### Roles

A role represents a set of permissions within your system.

```python
<!-- Code example in Python -->
@strawberry.type
class Role:
    id: strawberry.ID
    name: str
    description: str | None
    parent_role_id: strawberry.ID | None  # Optional inheritance
    is_system: bool  # Cannot be deleted
    tenant_id: strawberry.ID | None  # Multi-tenancy support
    created_at: datetime
    updated_at: datetime
```text
<!-- Code example in TEXT -->

**System Roles** (predefined):

- `admin` - Full system access
- `user` - Basic user access
- `guest` - Limited access

**Custom Roles**:

- Department-specific (e.g., `sales_manager`, `engineering_lead`)
- Feature-specific (e.g., `analytics_viewer`, `report_editor`)

### Permissions

A permission is a pairing of **resource** and **action**.

```python
<!-- Code example in Python -->
@strawberry.type
class Permission:
    id: strawberry.ID
    resource: str  # e.g., "user", "product", "order"
    action: str    # e.g., "create", "read", "update", "delete"
    description: str | None
    constraints: dict | None  # Optional JSONB constraints
```text
<!-- Code example in TEXT -->

**Standard Permissions**:

```text
<!-- Code example in TEXT -->
user.create, user.read, user.update, user.delete
product.create, product.read, product.update, product.delete
order.create, order.read, order.update, order.delete
```text
<!-- Code example in TEXT -->

**Constraints** (optional JSONB):

```json
<!-- Code example in JSON -->
{
  "own_data_only": true,
  "max_records": 1000,
  "time_restricted": "9-17",
  "department_only": "engineering"
}
```text
<!-- Code example in TEXT -->

### User Roles

Assignment of roles to users, with optional expiration.

```python
<!-- Code example in Python -->
@strawberry.type
class UserRole:
    id: strawberry.ID
    user_id: strawberry.ID
    role_id: strawberry.ID
    tenant_id: strawberry.ID | None
    expires_at: datetime | None
    granted_by: strawberry.ID  # Who granted this role
    created_at: datetime
```text
<!-- Code example in TEXT -->

---

## Permission Resolution

### How Permissions Are Resolved

```text
<!-- Code example in TEXT -->
User Request
    ↓
Check request-level cache
    ├─ HIT → Return permissions (< 1 µs)
    └─ MISS → Check PostgreSQL cache
            ├─ HIT → Load to request cache (0.1-0.3ms)
            └─ MISS → Resolve from role hierarchy
                    ├─ Fetch user's roles (0.1ms)
                    ├─ Fetch all inherited roles (via CTE) (1-10ms)
                    ├─ Fetch all permissions (0.5-2ms)
                    ├─ Cache in request memory (< 1 µs)
                    └─ Cache in PostgreSQL (< 1ms)

Total uncached: 2-15ms (depends on hierarchy depth)
Total cached: < 0.5ms
```text
<!-- Code example in TEXT -->

### API Methods

```python
<!-- Code example in Python -->
# Get all permissions for a user
permissions = await resolver.get_user_permissions(
    user_id="user-123",
    tenant_id="tenant-456",  # Optional, for multi-tenant
    use_cache=True           # Use caching (recommended)
)

# Check single permission
has_perm = await resolver.has_permission(
    user_id="user-123",
    resource="user",
    action="update"
)

# Check with exception
await resolver.check_permission(
    user_id="user-123",
    resource="order",
    action="delete",
    raise_on_deny=True  # Raises PermissionDenied if false
)

# Get user's direct roles
roles = await resolver.get_user_roles(
    user_id="user-123",
    tenant_id="tenant-456"  # Optional
)

# Get all permissions for a role (including inherited)
perms = await resolver.get_role_permissions(
    role_id="role-789",
    include_inherited=True
)
```text
<!-- Code example in TEXT -->

---

## Role Hierarchy Management

### Creating Role Hierarchies

```python
<!-- Code example in Python -->
# Define roles via GraphQL mutations
mutation CreateRoles {
  # Create system admin role
  createRole(name: "admin", isSystem: true) {
    id
    name
  }

  # Create department roles
  createRole(name: "sales_team", parentRoleId: "user") {
    id
  }

  createRole(name: "sales_manager", parentRoleId: "sales_team") {
    id
  }

  createRole(name: "sales_director", parentRoleId: "sales_manager") {
    id
  }
}
```text
<!-- Code example in TEXT -->

**Inheritance Chain**:

```text
<!-- Code example in TEXT -->
admin (system role)
  ↑
user (system role)
  ↑
sales_team
  ↑
sales_manager
  ↑
sales_director
```text
<!-- Code example in TEXT -->

A `sales_director` inherits all permissions from:

- `sales_director` (direct)
- `sales_manager` (parent)
- `sales_team` (grandparent)
- `user` (great-grandparent)
- `admin` (great-great-grandparent)

### Assigning Roles to Users

```python
<!-- Code example in Python -->
mutation AssignRole {
  assignRoleToUser(
    userId: "user-123"
    roleId: "sales_manager"
    tenantId: "tenant-456"
    expiresAt: "2025-12-31T23:59:59Z"
  ) {
    id
    user {
      id
      name
    }
    role {
      id
      name
    }
    expiresAt
  }
}
```text
<!-- Code example in TEXT -->

**Expiration**: Optional time-based role revocation.

### Querying Role Hierarchy

```python
<!-- Code example in Python -->
query GetRoleHierarchy {
  role(id: "role-789") {
    id
    name
    parentRole {
      id
      name
    }
    childRoles {
      id
      name
    }
    permissions {
      id
      resource
      action
    }
    ancestors {
      id
      name
    }
    descendants {
      id
      name
    }
  }
}
```text
<!-- Code example in TEXT -->

---

## Field-Level Authorization

### Using Directives

FraiseQL provides GraphQL directives for field-level access control.

#### `@requires_permission` Directive

```python
<!-- Code example in Python -->
@strawberry.type
class User:
    id: strawberry.ID
    name: str

    @requires_permission("user", "read_email")
    def email(self) -> str:
        """Only accessible to users with user:read_email permission"""
        return self.email_value

    @requires_permission("user", "read_salary")
    def salary(self) -> float:
        """Only accessible to users with user:read_salary permission"""
        return self.salary_value
```text
<!-- Code example in TEXT -->

**Behavior**:

- Field check: Permission required before field resolution
- Missing permission: Returns GraphQL error, field returns null
- No exception thrown: Graceful field hiding

#### `@requires_role` Directive

```python
<!-- Code example in Python -->
@strawberry.type
class Product:
    id: strawberry.ID
    name: str

    @requires_role("admin")
    def cost(self) -> float:
        """Only admins can see cost"""
        return self.cost_value

    @requires_role("sales_manager")
    def margin(self) -> float:
        """Only sales managers can see margin"""
        return (self.price - self.cost) / self.price
```text
<!-- Code example in TEXT -->

**Behavior**:

- Role check: User must have specified role
- Multiple roles (OR logic): `@requires_role(roles=["admin", "manager"])`
- If denied: Graceful null return with error

### Field Filtering Response

When a user doesn't have permission for a field:

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "user-123") {
    id        # ✓ Always included
    name      # ✓ Always included
    email     # ✗ HIDDEN - user lacks "user:read_email"
    salary    # ✗ HIDDEN - user lacks "user:read_salary"
  }
}
```text
<!-- Code example in TEXT -->

**Response**:

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "user-123",
      "name": "John Doe",
      "email": null,
      "salary": null
    }
  },
  "errors": [{
    "message": "Permission denied: user:read_email",
    "path": ["user", "email"]
  }]
}
```text
<!-- Code example in TEXT -->

---

## Row-Level Security (RLS)

### Automatic Row Filtering

Row-level security automatically filters query results based on user permissions.

```python
<!-- Code example in Python -->
# Install Rust row constraint resolver
from FraiseQL.enterprise.rbac.rust_row_constraints import RustRowConstraintResolver

row_resolver = RustRowConstraintResolver(
    db_pool=db_pool,
    cache_capacity=10000  # LRU cache size
)

# Add to schema
schema = strawberry.Schema(
    query=Query,
    mutation=Mutation,
    extensions=[create_rbac_middleware(row_constraint_resolver=row_resolver)]
)
```text
<!-- Code example in TEXT -->

### Row Constraints

Define what rows each role can access:

```python
<!-- Code example in Python -->
# Example: Employees can only see their own data
constraint = RowConstraint(
    role_id="employee",
    table_name="users",
    where_clause="owner_id = current_user_id"  # Parameterized
)

# Example: Managers see their department
constraint = RowConstraint(
    role_id="department_manager",
    table_name="employees",
    where_clause="department_id = (SELECT department_id FROM users WHERE id = current_user_id)"
)
```text
<!-- Code example in TEXT -->

### Performance

Row constraint checking with Rust FFI:

- **Cached lookup**: < 0.1ms (LRU cache)
- **Uncached lookup**: < 1ms (database query)
- **Supports 10,000+ rows**: Transparent filtering

**Example Query**:

```graphql
<!-- Code example in GraphQL -->
query {
  users {
    id
    name
    salary  # Only included if has permission
  }
}
```text
<!-- Code example in TEXT -->

**Behind the scenes**:

```sql
<!-- Code example in SQL -->
SELECT id, name, salary
FROM users
WHERE department_id = ? -- Automatically added by Rust resolver
```text
<!-- Code example in TEXT -->

---

## Multi-Tenant RBAC

### Tenant-Scoped Roles

Each role can be scoped to a tenant:

```python
<!-- Code example in Python -->
# Global role (NULL tenant_id)
role = {
    "id": "role-1",
    "name": "admin",
    "tenant_id": None,
    "permissions": [...]
}

# Tenant-specific role
role = {
    "id": "role-2",
    "name": "tenant_admin",
    "tenant_id": "tenant-123",
    "permissions": [...]
}
```text
<!-- Code example in TEXT -->

### Permission Resolution with Tenants

```python
<!-- Code example in Python -->
# Get permissions scoped to tenant
permissions = await resolver.get_user_permissions(
    user_id="user-123",
    tenant_id="tenant-456"  # Filter to this tenant
)

# Query with tenant context
await resolver.check_permission(
    user_id="user-123",
    resource="product",
    action="read",
    tenant_id="tenant-456"  # Tenant isolation
)
```text
<!-- Code example in TEXT -->

**Isolation**:

- User A (tenant-1) cannot inherit permissions from tenant-2 roles
- Row-level filtering automatically includes tenant context
- Cache keys include tenant_id

---

## GraphQL Mutations

### Creating Roles

```graphql
<!-- Code example in GraphQL -->
mutation {
  createRole(
    name: "content_manager"
    description: "Can manage all content"
    parentRoleId: "user"
    tenantId: "tenant-123"
  ) {
    id
    name
    parentRole { id name }
  }
}
```text
<!-- Code example in TEXT -->

### Assigning Permissions

```graphql
<!-- Code example in GraphQL -->
mutation {
  grantPermissionToRole(
    roleId: "role-456"
    resourceId: "resource-789"
    action: "create"
  ) {
    id
    role { name }
    permission { resource action }
  }
}
```text
<!-- Code example in TEXT -->

### Managing User Roles

```graphql
<!-- Code example in GraphQL -->
mutation {
  assignRoleToUser(
    userId: "user-123"
    roleId: "content_manager"
    tenantId: "tenant-456"
    expiresAt: "2025-12-31"
  ) {
    id
    user { name }
    role { name }
    expiresAt
  }
}

mutation {
  revokeRoleFromUser(
    userId: "user-123"
    roleId: "content_manager"
    tenantId: "tenant-456"
  ) {
    success
  }
}
```text
<!-- Code example in TEXT -->

---

## Performance Characteristics

### Permission Lookup Performance

| Scenario | Latency | Notes |
|----------|---------|-------|
| Request cache hit | < 1 µs | Already computed this request |
| PostgreSQL cache hit | 0.1-0.3ms | UNLOGGED table lookup |
| Role hierarchy computation | 2-15ms | CTE with up to 10 levels |
| **Total (cached)** | **< 0.5ms** | 99.9% of requests |
| **Total (uncached)** | **2-15ms** | Cold start or cache expired |

### Row Constraint Performance

| Operation | Latency | Notes |
|-----------|---------|-------|
| Cached constraint lookup | < 0.1ms | LRU cache (10,000 entries) |
| Uncached constraint lookup | < 1ms | Database query |
| WHERE clause generation | < 0.1ms | Template substitution |
| Query execution with constraints | < actual query time | Transparent to client |

### Scalability

- **Users**: 10,000+ concurrent
- **Roles per user**: Up to 100 (practical limit)
- **Role depth**: Up to 10 levels (architectural limit)
- **Permissions**: Unlimited (scaled by hardware)

---

## Best Practices

### 1. Cache at Request Level

Always reuse resolved permissions within a request:

```python
<!-- Code example in Python -->
# GOOD - Single resolution
user_permissions = await resolver.get_user_permissions(user_id)
has_create = "resource:create" in user_permissions
has_read = "resource:read" in user_permissions
has_update = "resource:update" in user_permissions

# BAD - Multiple database calls
has_create = await resolver.has_permission(user_id, "resource", "create")
has_read = await resolver.has_permission(user_id, "resource", "read")
has_update = await resolver.has_permission(user_id, "resource", "update")
```text
<!-- Code example in TEXT -->

### 2. Use Domain Versioning

Domain versioning automatically handles cache invalidation - don't manually clear caches:

```python
<!-- Code example in Python -->
# GOOD - Let domain versioning handle invalidation
mutation {
  createRole(name: "new_role") {
    id
  }
}
# Automatically increments domain_version for 'role' domain

# BAD - Manual cache management
cache.invalidate_all()  # Throws away valid data
```text
<!-- Code example in TEXT -->

### 3. Prefer Inheritance Over Duplication

Build role hierarchies rather than copying permissions:

```python
<!-- Code example in Python -->
# GOOD - Inheritance
user → team_lead → team_manager → director

# BAD - Duplication
user (has all permissions copied)
team_lead (has all same permissions again)
```text
<!-- Code example in TEXT -->

### 4. Set Expiration Dates

Use role expiration for temporary assignments:

```graphql
<!-- Code example in GraphQL -->
mutation {
  assignRoleToUser(
    userId: "contractor-123"
    roleId: "developer"
    expiresAt: "2025-03-31"  # Auto-revoke after contract
  ) {
    id
  }
}
```text
<!-- Code example in TEXT -->

### 5. Audit Role Changes

Log who made what changes:

```python
<!-- Code example in Python -->
# Automatically captured in audit logging
granted_by: "admin-user-456"  # Who granted the role
created_at: "2025-01-11T10:30:00Z"
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### Permission Not Working

1. Check role inheritance:

   ```graphql
<!-- Code example in GraphQL -->
   query {
     user(id: "user-123") {
       roles {
         name
         parentRole { name }
         permissions { resource action }
       }
     }
   }
   ```text
<!-- Code example in TEXT -->

2. Verify permission assignment:

   ```graphql
<!-- Code example in GraphQL -->
   query {
     role(id: "role-456") {
       permissions { resource action }
     }
   }
   ```text
<!-- Code example in TEXT -->

3. Check cache version:

   ```sql
<!-- Code example in SQL -->
   SELECT * FROM domain_versions WHERE domain = 'role';
   ```text
<!-- Code example in TEXT -->

### High Latency on Permission Checks

1. Check cache hit ratio:

   ```sql
<!-- Code example in SQL -->
   SELECT * FROM permission_cache_stats;
   ```text
<!-- Code example in TEXT -->

2. Verify domain versioning is working:

   ```sql
<!-- Code example in SQL -->
   SELECT version FROM domain_versions WHERE domain = 'role';
   -- Should be same across requests unless roles changed
   ```text
<!-- Code example in TEXT -->

3. Monitor role hierarchy depth:

   ```sql
<!-- Code example in SQL -->
   SELECT role_id, max_depth FROM role_hierarchy_depths;
   -- Limit to <10 for optimal performance
   ```text
<!-- Code example in TEXT -->

---

## Summary

FraiseQL RBAC provides:

✅ **Hierarchical roles** with up to 10 levels
✅ **Two-layer caching** for sub-millisecond lookups
✅ **Automatic cache invalidation** via domain versioning
✅ **Field-level authorization** with GraphQL directives
✅ **Row-level security** for transparent filtering
✅ **Multi-tenant support** with tenant-scoped roles
✅ **Production-grade performance** (<0.5ms cached)
✅ **Full audit trail** of role changes

Start with role hierarchy design, leverage automatic caching, and use expiration dates for temporary assignments.
