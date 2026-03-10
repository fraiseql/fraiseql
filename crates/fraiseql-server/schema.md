# FraiseQL RBAC & Audit Schema Documentation

**Phase**: 11.6 - Database Schema & Migrations
**Status**: ✅ Complete
**Test Coverage**: 22 verification tests

---

## Schema Overview

The RBAC and audit logging system consists of three main components:

1. **Audit Logging** (`audit_log` table)
   - Tracks all system events for compliance
   - Multi-tenant aware
   - Optimized for time-range queries

2. **Multi-Tenancy** (`tenants` table)
   - Provides tenant isolation
   - Enables row-level security
   - Integrates with users and audit_log

3. **RBAC** (Role-Based Access Control)
   - `roles` - Tenant-specific role definitions
   - `permissions` - Global permission registry
   - `role_permissions` - Role-to-permission assignments
   - `user_roles` - User-to-role assignments

---

## Table Structures

### audit_log

Comprehensive event logging for compliance and auditing.

```sql
CREATE TABLE audit_log (
    id UUID PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255),
    username VARCHAR(255),
    ip_address INET,
    resource_type VARCHAR(255),
    resource_id VARCHAR(255),
    action VARCHAR(255),
    before_state JSONB,
    after_state JSONB,
    status VARCHAR(50) NOT NULL,        -- success, failure, denied
    error_message TEXT,
    tenant_id UUID,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL
);
```

**Indexes** (7 total):

- `idx_audit_log_timestamp` - Time-based queries
- `idx_audit_log_user_id` - User activity tracking
- `idx_audit_log_event_type` - Event filtering
- `idx_audit_log_status` - Success/failure analysis
- `idx_audit_log_tenant_id` - Tenant isolation
- `idx_audit_log_composite` - Tenant + time queries
- `idx_audit_log_event_time` - Event type + time queries

**Use Cases**:

- Query all events in a time range: `timestamp DESC`
- Find all actions by a user: `user_id`
- Track event type distribution: `event_type`
- Monitor failed operations: `status = 'failure'`
- Tenant compliance queries: `tenant_id, timestamp DESC`

---

### tenants

Multi-tenant isolation and management.

```sql
CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    slug VARCHAR(255) UNIQUE,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    metadata JSONB,
    is_active BOOLEAN DEFAULT true
);
```

**Indexes** (3 total):

- `idx_tenants_name` - Tenant lookup
- `idx_tenants_slug` - URL-friendly access
- `idx_tenants_is_active` - Active tenant filtering

**Relationships**:

- 1:N with `users` (users.tenant_id → tenants.id)
- 1:N with `roles` (roles.tenant_id → tenants.id)
- 1:N with `audit_log` (audit_log.tenant_id → tenants.id)

---

### roles

Tenant-specific role definitions for RBAC.

```sql
CREATE TABLE roles (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    level INT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(tenant_id, name)
);
```

**Indexes** (3 total):

- `idx_roles_tenant_id` - Tenant-specific queries
- `idx_roles_name` - Role lookup
- `idx_roles_tenant_name` - Tenant + name composite

**Key Features**:

- Tenant-scoped (same role name allowed in different tenants)
- Level-based hierarchy (0 = admin, 100 = user, 200 = guest)
- Supports custom intermediate levels

**Role Hierarchy** (Example):

```
Level 0:   Admin (all permissions)
Level 50:  Moderator (limited admin)
Level 100: User (query + limited mutations)
Level 200: Guest (read-only queries)
```

---

### permissions

Global permission registry (not tenant-specific).

```sql
CREATE TABLE permissions (
    id UUID PRIMARY KEY,
    resource VARCHAR(255) NOT NULL,
    action VARCHAR(255) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE(resource, action)
);
```

**Indexes** (2 total):

- `idx_permissions_resource` - Resource filtering
- `idx_permissions_resource_action` - Composite lookup

**Default Permissions** (14 pre-populated):

- `query:read` - Execute read queries
- `mutation:write` - Execute mutations
- `admin:read`, `admin:write` - Admin operations
- `audit:read`, `audit:write` - Audit log access
- `rbac:read`, `rbac:write` - RBAC configuration
- `cache:read`, `cache:write` - Cache management
- `config:read`, `config:write` - Configuration access
- `federation:read`, `federation:write` - Federation operations

**Format**: `resource:action` (e.g., `query:read`)

---

### role_permissions

Many-to-many junction table linking roles to permissions.

```sql
CREATE TABLE role_permissions (
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (role_id, permission_id)
);
```

**Indexes** (2 total):

- `idx_role_permissions_role_id` - Permissions for role
- `idx_role_permissions_permission_id` - Roles with permission

**Cascade Behavior**:

- When role deleted: All role_permissions entries deleted
- When permission deleted: All role_permissions entries deleted

---

### user_roles

Many-to-many junction table linking users to roles.

```sql
CREATE TABLE user_roles (
    user_id VARCHAR(255) NOT NULL,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, role_id, tenant_id),
    UNIQUE(user_id, role_id)
);
```

**Indexes** (4 total):

- `idx_user_roles_user_id` - Roles for user
- `idx_user_roles_role_id` - Users with role
- `idx_user_roles_tenant_id` - Tenant filtering
- `idx_user_roles_user_tenant` - User + tenant queries

**Key Features**:

- Composite tenant context (user can have different roles per tenant)
- Tracks assignment time for audit trail
- Cascading deletion protects referential integrity

---

## Data Relationships

```
┌─────────────────────┐
│     tenants         │
├─────────────────────┤
│ id (PK)             │
│ name (UNIQUE)       │
│ slug (UNIQUE)       │
│ description         │
│ created_at          │
│ updated_at          │
│ metadata (JSONB)    │
│ is_active           │
└──────────┬──────────┘
           │
           │ 1:N
           ▼
┌─────────────────────┐         ┌──────────────────────┐
│       roles         │◄───────►│    permissions       │
├─────────────────────┤  N:M    ├──────────────────────┤
│ id (PK)             │    via  │ id (PK)              │
│ tenant_id (FK)      │  role_  │ resource             │
│ name (UNIQUE/TID)   │ permis- │ action               │
│ description         │ sions   │ description          │
│ level               │         │ created_at           │
│ created_at          │         └──────────────────────┘
│ updated_at          │
└──────────┬──────────┘
           │
      N:M  │
       via │ user_roles
           ▼
┌──────────────────────────────┐
│       users                  │
├──────────────────────────────┤
│ id (PK)                      │
│ name                         │
│ email                        │
│ tenant_id (FK)               │
│ password_hash                │
│ created_at                   │
└──────────────────────────────┘

┌──────────────────────────────┐
│       audit_log              │
├──────────────────────────────┤
│ id (PK)                      │
│ timestamp (indexed)          │
│ event_type (indexed)         │
│ user_id (indexed)            │
│ username                     │
│ ip_address                   │
│ resource_type                │
│ resource_id                  │
│ action                       │
│ before_state (JSONB)         │
│ after_state (JSONB)          │
│ status (indexed)             │
│ error_message                │
│ tenant_id (FK, indexed)      │
│ metadata (JSONB)             │
│ created_at                   │
└──────────────────────────────┘
```

---

## Migration Files

### 0010_audit_log.sql

- Creates audit_log table with 16 columns
- Adds 7 performance-optimized indexes
- Supports JSON before/after state tracking

### 0011_tenants.sql

- Creates tenants table
- Adds tenant_id to users table (if exists)
- Adds tenant_id to audit_log table
- Establishes foreign key relationships

### 0012_rbac.sql

- Creates roles table (tenant-scoped)
- Creates permissions table (global)
- Creates role_permissions junction table
- Creates user_roles junction table
- Pre-populates 14 default system permissions

---

## Key Design Decisions

### 1. Audit Log Tracking

- JSONB columns for flexible before/after state
- Separate status field for quick filtering (success/failure/denied)
- Tenant-aware for multi-tenant compliance

### 2. Role Hierarchy

- Numeric levels allow arbitrary hierarchy
- UNIQUE(tenant_id, name) enables same role names across tenants
- Separate from permissions for flexibility

### 3. Global Permissions

- Permissions are not tenant-scoped (reused across all tenants)
- Resource:action format is convention, not enforced
- Default permissions provide starting set

### 4. Cascade Deletes

- Deleting role cascades to role_permissions and user_roles
- Deleting tenant cascades to roles and users
- Deleting audit events: SET NULL for tenant_id (preserve event history)

### 5. Composite Indexes

- tenant_id + timestamp for multi-tenant time-range queries
- event_type + timestamp for filtering by event and time
- user_id + tenant_id for user activity within tenant

### 6. Idempotent Migrations

- All CREATE statements use IF NOT EXISTS
- Column additions use DO blocks to check existence first
- Allows safe re-running and partial recovery

---

## Query Patterns

### Find User's Roles in Tenant

```sql
SELECT r.*
FROM roles r
JOIN user_roles ur ON r.id = ur.role_id
WHERE ur.user_id = $1
  AND ur.tenant_id = $2
ORDER BY r.name;
```

### Get Permissions for User

```sql
SELECT DISTINCT p.*
FROM permissions p
JOIN role_permissions rp ON p.id = rp.permission_id
JOIN roles r ON rp.role_id = r.id
JOIN user_roles ur ON r.id = ur.role_id
WHERE ur.user_id = $1
  AND ur.tenant_id = $2;
```

### Audit Events in Time Range

```sql
SELECT *
FROM audit_log
WHERE tenant_id = $1
  AND timestamp BETWEEN $2 AND $3
ORDER BY timestamp DESC
LIMIT $4 OFFSET $5;
```

### Failed Operations by User

```sql
SELECT *
FROM audit_log
WHERE user_id = $1
  AND status = 'failure'
  AND tenant_id = $2
ORDER BY timestamp DESC;
```

---

## Performance Notes

### Indexes

- 7 indexes on audit_log support common query patterns
- Composite indexes (tenant_id, timestamp) reduce query plans
- JSONB column doesn't require index for basic queries

### Scaling

- Audit log can grow quickly; consider time-based partitioning for large datasets
- Tenant_id index prevents cross-tenant data leaks
- Role_permissions junction is small (typically <1000 entries per tenant)

### Optimization Opportunities

- Archive audit logs older than 1 year to separate table
- Create materialized view for user permissions (if queries slow)
- Consider partial indexes on audit_log for common filters

---

## Testing

22 schema verification tests cover:

- ✅ All table structures and columns
- ✅ Index existence and naming
- ✅ Foreign key relationships
- ✅ Cascade delete constraints
- ✅ Tenant isolation enforcement
- ✅ Permission default values
- ✅ Migration idempotency
- ✅ Composite indexes for performance

**Test Categories**:

- Table structure verification (5 tests)
- Index verification (6 tests)
- Relationship verification (3 tests)
- Constraint verification (3 tests)
- Configuration verification (5 tests)

---

## Maintenance

### Regular Tasks

1. Monitor audit_log growth (may require archival)
2. Verify index performance with EXPLAIN ANALYZE
3. Check for orphaned records (data integrity)

### Admin Commands

```sql
-- Vacuum and analyze for query optimization
VACUUM ANALYZE audit_log;
VACUUM ANALYZE roles;

-- Check index size
SELECT schemaname, tablename, indexname, pg_size_pretty(pg_relation_size(indexrelid)) as size
FROM pg_stat_user_indexes
WHERE tablename IN ('audit_log', 'roles', 'permissions');

-- Find slow audit queries
SELECT * FROM pg_stat_statements
WHERE query LIKE '%audit_log%'
ORDER BY mean_exec_time DESC;
```

---

**Last Updated**: 2026-02-04
**Phase**: 11.6 - Database Schema & Migrations
**Status**: ✅ Complete
