# Trinity Pattern - FraiseQL's Database Architecture

**Time to Complete:** 10-15 minutes
**Prerequisites:** Basic PostgreSQL knowledge, understanding of GraphQL concepts

## Overview

The Trinity Pattern is FraiseQL's core database architecture that provides **zero-copy views**, **automatic multi-tenancy**, and **consistent naming conventions**. It consists of three layers for each entity:

1. **Base Table (`tb_*`)** - Raw data storage with tenant isolation
2. **View (`v_*`)** - GraphQL API layer with automatic filtering
3. **Computed View (`tv_*`)** - Pre-joined data for complex queries

## Why "Trinity"?

The pattern creates three objects per entity, working in harmony:

```
tb_user (base table) → v_user (API view) → tv_user_with_posts (computed view)
```

This three-tier approach gives you:
- **Performance** (no expensive JOINs in queries)
- **Security** (automatic tenant isolation)
- **Flexibility** (easy to extend without breaking APIs)

---

## The Three Layers

### 1. Base Tables (`tb_*`)

**Purpose**: Raw data storage with tenant isolation

**Naming Convention**: `tb_{entity}` (e.g., `tb_user`, `tb_post`, `tb_comment`)

**Structure**:
```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Tenant isolation index
CREATE INDEX idx_user_tenant ON tb_user(tenant_id);
```

**Key Features**:
- **Tenant ID** always present for multi-tenancy
- **JSONB data** column for flexible schema
- **Audit timestamps** (created_at, updated_at)
- **No foreign keys** in base tables (handled in views)

### 2. API Views (`v_*`)

**Purpose**: GraphQL API layer with automatic security filtering

**Naming Convention**: `v_{entity}` (e.g., `v_user`, `v_post`, `v_comment`)

**Structure**:
```sql
CREATE VIEW v_user AS
SELECT
    id,
    tenant_id,
    data->>'email' as email,
    data->>'first_name' as first_name,
    data->>'last_name' as last_name,
    data,
    created_at,
    updated_at
FROM tb_user
WHERE tenant_id = current_setting('app.tenant_id')::uuid;
```

**Key Features**:
- **Automatic tenant filtering** via session variables
- **Flattened JSONB fields** for GraphQL compatibility
- **Security by default** (impossible to query other tenants)
- **No performance overhead** (views are optimized in PostgreSQL)

### 3. Computed Views (`tv_*`)

**Purpose**: Pre-joined data for complex queries, avoiding runtime JOINs

**Naming Convention**: `tv_{entity}_{relationship}` (e.g., `tv_user_with_posts`, `tv_post_with_comments`)

**Structure**:
```sql
CREATE VIEW tv_user_with_posts AS
SELECT
    u.id,
    u.tenant_id,
    u.data->>'email' as email,
    u.data->>'first_name' as first_name,
    u.data->>'last_name' as last_name,
    jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.data->>'title',
            'content', p.data->>'content',
            'created_at', p.created_at
        ) ORDER BY p.created_at DESC
    ) FILTER (WHERE p.id IS NOT NULL) as posts,
    u.created_at,
    u.updated_at
FROM v_user u
LEFT JOIN v_post p ON p.data->>'user_id' = u.id::text
GROUP BY u.id, u.tenant_id, u.email, u.first_name, u.last_name, u.created_at, u.updated_at;
```

**Key Features**:
- **Pre-joined data** (no expensive JOINs at query time)
- **Aggregated relationships** (posts as JSON array)
- **Zero-copy performance** (data prepared once, read many times)
- **GraphQL-optimized** structure

---

## Benefits of the Trinity Pattern

### 1. Zero-Copy Performance

**Traditional Approach** (expensive JOINs):
```sql
-- Runtime JOIN for every query
SELECT u.*, p.*
FROM users u
JOIN posts p ON p.user_id = u.id
WHERE u.id = $1;
```

**Trinity Pattern** (pre-computed):
```sql
-- Single table scan, no JOINs
SELECT *
FROM tv_user_with_posts
WHERE id = $1;
```

**Performance Impact**: 10-100x faster for complex queries

### 2. Automatic Multi-Tenancy

**Session Variable Injection**:
```python
# FraiseQL automatically sets tenant from JWT
# SET LOCAL app.tenant_id = 'tenant-uuid';
```

**View-Level Security**:
```sql
CREATE VIEW v_user AS
SELECT * FROM tb_user
WHERE tenant_id = current_setting('app.tenant_id')::uuid;
-- Impossible to query other tenants!
```

### 3. Schema Evolution Without Migrations

**Add New Fields**:
```sql
-- No ALTER TABLE needed!
UPDATE tb_user
SET data = jsonb_set(data, '{new_field}', '"new_value"')
WHERE id = $1;
```

**GraphQL Schema Updates**:
```python
@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    email: str
    first_name: str
    last_name: str
    new_field: str | None = None  # Added without database migration
```

### 4. Consistent Naming Conventions

**Always Use**:
- `tb_user` - Base table
- `v_user` - API view
- `tv_user_with_posts` - Computed view

**Never Use**:
- `users` - Ambiguous, no tenant context
- `user_view` - Inconsistent naming
- `user_posts` - Missing computed view prefix

---

## Implementation Guide

### Step 1: Create Base Table

```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_user_tenant ON tb_user(tenant_id);
CREATE INDEX idx_user_email ON tb_user USING GIN ((data->'email'));
```

### Step 2: Create API View

```sql
CREATE VIEW v_user AS
SELECT
    id,
    tenant_id,
    data->>'email' as email,
    data->>'first_name' as first_name,
    data->>'last_name' as last_name,
    data,
    created_at,
    updated_at
FROM tb_user
WHERE tenant_id = current_setting('app.tenant_id')::uuid;
```

### Step 3: Create Computed View (Optional)

```sql
CREATE VIEW tv_user_with_posts AS
SELECT
    u.id,
    u.tenant_id,
    u.data->>'email' as email,
    u.data->>'first_name' as first_name,
    u.data->>'last_name' as last_name,
    jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.data->>'title',
            'created_at', p.created_at
        ) ORDER BY p.created_at DESC
    ) FILTER (WHERE p.id IS NOT NULL) as posts,
    u.created_at,
    u.updated_at
FROM v_user u
LEFT JOIN v_post p ON p.data->>'user_id' = u.id::text
GROUP BY u.id, u.tenant_id, u.email, u.first_name, u.last_name, u.created_at, u.updated_at;
```

### Step 4: Use in FraiseQL

```python
import fraiseql

@fraiseql.type(sql_source="v_user")
class User:
    """User account."""
    id: UUID
    email: str
    first_name: str
    last_name: str
    created_at: datetime

@fraiseql.type(sql_source="tv_user_with_posts")
class UserWithPosts:
    """User with their posts."""
    id: UUID
    email: str
    first_name: str
    last_name: str
    posts: list[Post]
    created_at: datetime

@fraiseql.query
async def user_with_posts(info, id: UUID) -> UserWithPosts:
    """Get user with all their posts."""
    db = info.context["db"]
    return await db.find_one("tv_user_with_posts", where={"id": id})
```

---

## Best Practices

### 1. Always Use Trinity Naming

```sql
-- ✅ Correct
CREATE TABLE tb_product (...);
CREATE VIEW v_product AS ...;
CREATE VIEW tv_product_with_categories AS ...;

-- ❌ Avoid
CREATE TABLE products (...);
CREATE VIEW product_view AS ...;
```

### 2. Keep Base Tables Simple

```sql
-- ✅ Base table with just essentials
CREATE TABLE tb_order (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- ❌ Don't add many columns to base table
CREATE TABLE tb_order (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL,
    customer_name VARCHAR(255),  -- Put in data JSONB instead
    order_total DECIMAL(10,2),   -- Put in data JSONB instead
    status VARCHAR(50),          -- Put in data JSONB instead
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### 3. Use Views for Business Logic

```sql
-- ✅ Business logic in views
CREATE VIEW v_order AS
SELECT
    id,
    tenant_id,
    data->>'customer_name' as customer_name,
    (data->>'order_total')::decimal as order_total,
    data->>'status' as status,
    created_at
FROM tb_order
WHERE tenant_id = current_setting('app.tenant_id')::uuid
  AND data->>'status' != 'cancelled';  -- Business rule
```

### 4. Create Computed Views for Common Queries

```sql
-- ✅ Pre-join frequently accessed data
CREATE VIEW tv_order_with_items AS
SELECT
    o.id,
    o.customer_name,
    o.order_total,
    jsonb_agg(
        jsonb_build_object(
            'product_name', i.data->>'name',
            'quantity', i.data->>'quantity',
            'price', i.data->>'price'
        )
    ) as items
FROM v_order o
LEFT JOIN v_order_item i ON i.data->>'order_id' = o.id::text
GROUP BY o.id, o.customer_name, o.order_total;
```

---

## Migration from Simple Tables

If you have existing tables using simple naming:

```sql
-- Step 1: Rename existing table
ALTER TABLE users RENAME TO tb_user;

-- Step 2: Add tenant column (if not present)
ALTER TABLE tb_user ADD COLUMN tenant_id UUID NOT NULL DEFAULT 'default-tenant';

-- Step 3: Create view
CREATE VIEW v_user AS
SELECT * FROM tb_user
WHERE tenant_id = current_setting('app.tenant_id')::uuid;

-- Step 4: Update application to use v_user
```

See [Migration Guide](./migrations.md) for detailed steps.

---

## Common Patterns

### 1. Entity with Relationships

```sql
-- Base tables
CREATE TABLE tb_user (...);
CREATE TABLE tb_post (...);

-- API views
CREATE VIEW v_user AS ...;
CREATE VIEW v_post AS ...;

-- Computed view
CREATE VIEW tv_user_with_posts AS
SELECT u.*, jsonb_agg(p.*) as posts
FROM v_user u
LEFT JOIN v_post p ON p.data->>'user_id' = u.id::text
GROUP BY u.id;
```

### 2. Hierarchical Data

```sql
-- Categories with subcategories
CREATE VIEW tv_category_with_subcategories AS
WITH RECURSIVE category_tree AS (
    SELECT c.*, 0 as depth
    FROM v_category c
    WHERE c.data->>'parent_id' IS NULL

    UNION ALL

    SELECT c.*, ct.depth + 1
    FROM v_category c
    JOIN category_tree ct ON c.data->>'parent_id' = ct.id::text
)
SELECT * FROM category_tree;
```

### 3. Aggregated Data

```sql
-- User with post counts and latest activity
CREATE VIEW tv_user_with_stats AS
SELECT
    u.*,
    COUNT(p.id) as post_count,
    MAX(p.created_at) as latest_post_at,
    COUNT(c.id) as comment_count
FROM v_user u
LEFT JOIN v_post p ON p.data->>'user_id' = u.id::text
LEFT JOIN v_comment c ON c.data->>'post_id' = p.id::text
GROUP BY u.id;
```

---

## Testing Your Trinity Pattern

### 1. Verify Tenant Isolation

```sql
-- Test: Can't query other tenants
SET LOCAL app.tenant_id = 'tenant-a';
SELECT COUNT(*) FROM v_user;  -- Should only show tenant-a users

SET LOCAL app.tenant_id = 'tenant-b';
SELECT COUNT(*) FROM v_user;  -- Should only show tenant-b users
```

### 2. Check Performance

```sql
-- Explain query plan
EXPLAIN ANALYZE SELECT * FROM tv_user_with_posts WHERE id = $1;
-- Should show "Index Scan" or "Seq Scan" but no "Hash Join"
```

### 3. Validate Data Integrity

```sql
-- Ensure views return expected data
SELECT
    (SELECT COUNT(*) FROM tb_user) as base_count,
    (SELECT COUNT(*) FROM v_user) as view_count,
    (SELECT COUNT(*) FROM tv_user_with_posts) as computed_count;
-- All counts should match (or computed_count <= base_count for filtered views)
```

---

## Next Steps

- [Database Naming Conventions](../database/table-naming-conventions.md) - Complete naming reference
- [Migration Guide](./migrations.md) - Migrate from simple tables
- [View Strategies](../database/view-strategies.md) - Advanced view patterns
- [Performance Tuning](../performance/performance-guide.md) - Optimize your trinity pattern

---

**Remember**: The Trinity Pattern (tb_ → v_ → tv_) is FraiseQL's foundation for secure, performant, scalable applications. Use it consistently for best results.
