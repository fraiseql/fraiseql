# Migrating from Simple Tables to Trinity Pattern

**Time to Complete:** 15-30 minutes  
**Prerequisites:** Basic PostgreSQL knowledge, existing database with simple table names  
**Target Audience:** Developers with existing FraiseQL applications using simple naming

## Overview

This guide helps you migrate from simple table naming (`users`, `posts`, `comments`) to FraiseQL's recommended **Trinity Pattern** (`tb_user`, `v_user`, `tv_user_with_posts`). The Trinity Pattern provides:

- **Performance**: 10-100x faster queries through pre-computed data
- **Consistency**: Clear separation of concerns (base tables, views, computed views)
- **Scalability**: Built for production workloads with automatic multi-tenancy

---

## When to Migrate

**Migrate when:**
- Your application has >10,000 rows per table
- Query performance is >5ms per request
- You need embedded relationships without JOINs
- You're preparing for production deployment

**Wait if:**
- You're in early prototype/MVP stage
- Dataset is <1,000 rows per table
- Performance is acceptable (<2ms per query)

---

## Migration Strategy

### Phase 1: Assessment (5 minutes)

**Step 1: Inventory Current Tables**
```sql
-- Find all tables without tb_ prefix
SELECT table_name, table_type
FROM information_schema.tables
WHERE table_schema = 'public'
  AND table_name NOT LIKE 'tb_%'
  AND table_name NOT LIKE 'v_%'
  AND table_name NOT LIKE 'tv_%'
  AND table_name NOT LIKE 'mv_%'
ORDER BY table_name;
```

**Step 2: Identify Foreign Key Relationships**
```sql
-- Map relationships between tables
SELECT
    tc.table_name,
    tc.constraint_name,
    ccu.table_name AS foreign_table_name,
    ccu.column_name AS foreign_column_name
FROM information_schema.table_constraints AS tc
JOIN information_schema.constraint_column_usage AS ccu
  ON ccu.constraint_name = tc.constraint_name
WHERE tc.constraint_type = 'FOREIGN KEY'
  AND tc.table_schema = 'public';
```

**Step 3: Check for Existing Views**
```sql
-- Find existing views that reference renamed tables
SELECT table_name, view_definition
FROM information_schema.views
WHERE view_schema = 'public'
  AND view_definition LIKE '%users%'
  OR view_definition LIKE '%posts%'
  OR view_definition LIKE '%comments%';
```

---

## Phase 2: Database Migration (10-15 minutes)

### Step 1: Rename Base Tables

**Basic Rename:**
```sql
-- Rename tables with tb_ prefix
ALTER TABLE users RENAME TO tb_user;
ALTER TABLE posts RENAME TO tb_post;
ALTER TABLE comments RENAME TO tb_comment;
```

**With Data Preservation:**
```sql
-- Safer approach with backup
BEGIN;

-- Create new tables with tb_ prefix
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT 'default-tenant',
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Migrate data
INSERT INTO tb_user (id, tenant_id, data, created_at, updated_at)
SELECT 
    id,
    COALESCE(tenant_id, 'default-tenant'),
    jsonb_build_object(
        'email', email,
        'first_name', first_name,
        'last_name', last_name,
        'created_at', created_at
    ),
    created_at,
    updated_at
FROM users;

-- Drop old table
DROP TABLE users;

COMMIT;
```

### Step 2: Create API Views (`v_*`)

**Simple Views:**
```sql
-- Create views for GraphQL API
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

CREATE VIEW v_post AS
SELECT
    id,
    tenant_id,
    data->>'title' as title,
    data->>'content' as content,
    data->>'user_id' as user_id,
    data,
    created_at,
    updated_at
FROM tb_post
WHERE tenant_id = current_setting('app.tenant_id')::uuid;
```

**With Multi-Tenancy:**
```sql
-- Views with automatic tenant isolation
CREATE OR REPLACE FUNCTION set_tenant_context()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        EXECUTE format('SET LOCAL app.tenant_id = %L', NEW.tenant_id);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply to all base tables
CREATE TRIGGER trg_set_tenant_user
BEFORE INSERT ON tb_user
FOR EACH ROW EXECUTE FUNCTION set_tenant_context();
```

### Step 3: Create Computed Views (`tv_*`)

**Basic Computed View:**
```sql
-- Table view with pre-computed relationships
CREATE TABLE tv_user_with_posts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sync function
CREATE OR REPLACE FUNCTION fn_sync_tv_user_with_posts(p_user_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user_with_posts (id, tenant_id, data)
    SELECT
        u.id,
        u.tenant_id,
        jsonb_build_object(
            'id', u.id,
            'email', u.data->>'email',
            'first_name', u.data->>'first_name',
            'last_name', u.data->>'last_name',
            'created_at', u.created_at,
            'posts', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', p.id,
                        'title', p.data->>'title',
                        'content', p.data->>'content',
                        'created_at', p.created_at
                    )
                    ORDER BY p.created_at DESC
                )
                FROM v_post p
                WHERE p.data->>'user_id' = u.id::text
                LIMIT 10
            )
        )
    FROM tb_user u
    WHERE u.id = p_user_id
    ON CONFLICT (id) DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;
```

**With Triggers for Automatic Sync:**
```sql
-- Triggers to keep tv_* in sync
CREATE OR REPLACE FUNCTION trg_sync_tv_user_with_posts()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM fn_sync_tv_user_with_posts(NEW.id);
    ELSIF TG_OP = 'UPDATE' THEN
        PERFORM fn_sync_tv_user_with_posts(NEW.id);
    ELSIF TG_OP = 'DELETE' THEN
        DELETE FROM tv_user_with_posts WHERE id = OLD.id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_sync_tv_user_with_posts
AFTER INSERT OR UPDATE OR DELETE ON tb_user
FOR EACH ROW EXECUTE FUNCTION trg_sync_tv_user_with_posts();

CREATE TRIGGER trg_sync_tv_user_with_posts_on_post
AFTER INSERT OR UPDATE OR DELETE ON tb_post
FOR EACH ROW EXECUTE FUNCTION trg_sync_tv_user_with_posts();
```

---

## Phase 3: Application Updates (5-10 minutes)

### Step 1: Update FraiseQL Types

**Before (Simple):**
```python
import fraiseql

@fraiseql.type(sql_source="users")
class User:
    id: UUID
    email: str
    first_name: str
    last_name: str
```

**After (Trinity):**
```python
import fraiseql

@fraiseql.type(sql_source="tv_user_with_posts", jsonb_column="data")
class UserWithPosts:
    id: UUID
    email: str
    first_name: str
    last_name: str
    posts: list[Post] | None = None

@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    email: str
    first_name: str
    last_name: str
```

### Step 2: Update Queries and Mutations

**Query Updates:**
```python
# Before
@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("users", id=id)

# After
@fraiseql.query
async def user_with_posts(info, id: UUID) -> UserWithPosts:
    db = info.context["db"]
    return await db.find_one("tv_user_with_posts", id=id)

@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("v_user", id=id)
```

**Mutation Updates:**
```python
# Before
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    result = await db.insert("users", input.dict())
    return result

# After
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    # Insert into base table
    result = await db.insert("tb_user", input.dict())
    
    # Sync computed view
    await db.call_function("fn_sync_tv_user_with_posts", {
        "p_user_id": result["id"]
    })
    
    # Return from API view
    return await db.find_one("v_user", id=result["id"])
```

---

## Phase 4: Testing and Verification (5 minutes)

### Step 1: Verify Data Integrity

**Check Row Counts:**
```sql
-- Verify all data migrated
SELECT 
    (SELECT COUNT(*) FROM tb_user) as tb_user_count,
    (SELECT COUNT(*) FROM v_user) as v_user_count,
    (SELECT COUNT(*) FROM tv_user_with_posts) as tv_user_count;

-- Should return: tb_user_count = v_user_count = tv_user_count
```

**Check Sample Data:**
```sql
-- Verify data structure
SELECT 
    u.data->>'email' as original_email,
    v.email as view_email,
    tv.data->>'email' as tv_email
FROM tb_user u
JOIN v_user v ON u.id = v.id
JOIN tv_user_with_posts tv ON u.id = tv.id
LIMIT 1;
```

### Step 2: Test GraphQL Operations

**Query Test:**
```graphql
query TestUserWithPosts($id: UUID!) {
  userWithPosts(id: $id) {
    id
    email
    firstName
    lastName
    posts {
      id
      title
      content
    }
  }
}
```

**Mutation Test:**
```graphql
mutation TestCreateUser($input: CreateUserInput!) {
  createUser(input: $input) {
    id
    email
    firstName
    lastName
  }
}
```

### Step 3: Performance Validation

**Before vs After:**
```sql
-- Test query performance
EXPLAIN ANALYZE SELECT * FROM users WHERE id = $1;
-- Expected: 5-10ms (table scan)

EXPLAIN ANALYZE SELECT * FROM tv_user_with_posts WHERE id = $1;
-- Expected: 0.05-0.5ms (indexed lookup)
```

---

## Common Edge Cases and Solutions

### Edge Case 1: Foreign Key Constraints

**Problem:** Foreign keys reference old table names
```sql
-- Before migration
ALTER TABLE posts ADD CONSTRAINT fk_user 
FOREIGN KEY (user_id) REFERENCES users(id);
```

**Solution:** Update foreign keys to use new table names
```sql
-- After migration
ALTER TABLE tb_post ADD CONSTRAINT fk_user 
FOREIGN KEY (user_id) REFERENCES tb_user(id);
```

### Edge Case 2: Existing Views and Functions

**Problem:** Views reference old table names
```sql
-- Existing view breaks after rename
CREATE VIEW user_summary AS
SELECT COUNT(*) FROM users;
```

**Solution:** Update all dependent objects
```sql
-- Update view to use new table name
CREATE OR REPLACE VIEW user_summary AS
SELECT COUNT(*) FROM tb_user;
```

### Edge Case 3: Application Code References

**Problem:** Hard-coded SQL in application code
```python
# Hard-coded reference breaks
cursor.execute("SELECT * FROM users WHERE id = %s", (user_id,))
```

**Solution:** Use FraiseQL repository pattern
```python
# Use FraiseQL abstraction
user = await db.find_one("v_user", id=user_id)
```

### Edge Case 4: Materialized Views

**Problem:** Materialized views depend on renamed tables
```sql
-- Materialized view breaks
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT COUNT(*) FROM users;
```

**Solution:** Refresh materialized views after migration
```sql
-- Update and refresh
CREATE OR REPLACE MATERIALIZED VIEW mv_user_stats AS
SELECT COUNT(*) FROM tb_user;

REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats;
```

---

## Rollback Plan

### If Migration Fails

**Step 1: Database Rollback**
```sql
-- Reverse table renames
ALTER TABLE tb_user RENAME TO users;
ALTER TABLE tb_post RENAME TO posts;
ALTER TABLE tb_comment RENAME TO comments;

-- Drop new objects
DROP VIEW IF EXISTS v_user;
DROP VIEW IF EXISTS v_post;
DROP TABLE IF EXISTS tv_user_with_posts;
```

**Step 2: Application Rollback**
```python
-- Revert type definitions
@fraiseql.type(sql_source="users")
class User:
    # ... original definition

# Revert queries/mutations
@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("users", id=id)
```

### Rollback Triggers

**Create rollback function:**
```sql
CREATE OR REPLACE FUNCTION fn_rollback_migration()
RETURNS TEXT AS $$
DECLARE
    v_result TEXT;
BEGIN
    -- Log rollback attempt
    INSERT INTO migration_log (action, status, created_at)
    VALUES ('rollback', 'started', NOW());
    
    -- Perform rollback
    BEGIN
        ALTER TABLE tb_user RENAME TO users;
        ALTER TABLE tb_post RENAME TO posts;
        ALTER TABLE tb_comment RENAME TO comments;
        
        DROP VIEW IF EXISTS v_user;
        DROP VIEW IF EXISTS v_post;
        DROP TABLE IF EXISTS tv_user_with_posts;
        
        v_result := 'success';
    EXCEPTION WHEN OTHERS THEN
        v_result := 'failed: ' || SQLERRM;
    END;
    
    UPDATE migration_log SET status = v_result, completed_at = NOW()
    WHERE action = 'rollback' AND status = 'started';
    
    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

---

## Migration Checklist

### Pre-Migration Checklist
- [ ] **Backup database** (`pg_dump fraiseql_db > backup.sql`)
- [ ] **Test on staging** (never migrate production directly)
- [ ] **Document current schema** (`pg_dump --schema-only > schema_before.sql`)
- [ ] **Identify all dependencies** (views, functions, triggers)
- [ ] **Schedule maintenance window** (allow 30 minutes downtime)

### Migration Checklist
- [ ] **Rename base tables** (`users` → `tb_user`)
- [ ] **Create API views** (`v_user`, `v_post`)
- [ ] **Create computed views** (`tv_user_with_posts`)
- [ ] **Add sync functions** (`fn_sync_tv_*`)
- [ ] **Create sync triggers** (automatic updates)
- [ ] **Update foreign keys** (reference new table names)
- [ ] **Update dependent objects** (views, functions)
- [ ] **Test data integrity** (row counts, sample data)

### Post-Migration Checklist
- [ ] **Update application code** (type definitions, queries, mutations)
- [ ] **Run test suite** (all tests pass)
- [ ] **Performance validation** (queries faster than before)
- [ ] **Monitor for errors** (check logs for 1 hour)
- [ ] **Update documentation** (API docs, READMEs)
- [ ] **Team training** (explain new pattern to developers)

---

## Troubleshooting

### Common Issues

**Issue 1: "relation "users" does not exist"**
- **Cause:** Application still references old table name
- **Solution:** Update all SQL queries and FraiseQL type definitions

**Issue 2: "foreign key violation"**
- **Cause:** Foreign keys still reference old table names
- **Solution:** Update foreign key constraints to use `tb_*` tables

**Issue 3: "view definition has changed"**
- **Cause:** Views depend on renamed tables
- **Solution:** Recreate all views with new table names

**Issue 4: Performance not improved**
- **Cause:** Not using `tv_*` computed views for queries
- **Solution:** Update GraphQL types to use `tv_*` tables for complex queries

### Debug Queries

**Check Migration Status:**
```sql
-- Verify all trinity objects exist
SELECT 
    'tb_user' in (SELECT table_name FROM information_schema.tables WHERE table_name = 'tb_user'),
    'v_user' in (SELECT table_name FROM information_schema.views WHERE view_name = 'v_user'),
    'tv_user_with_posts' in (SELECT table_name FROM information_schema.tables WHERE table_name = 'tv_user_with_posts');
```

**Check Sync Status:**
```sql
-- Verify computed views are in sync
SELECT 
    'user_sync' = CASE 
        WHEN (SELECT COUNT(*) FROM tb_user) = (SELECT COUNT(*) FROM tv_user_with_posts) 
        THEN 'OK' 
        ELSE 'OUT_OF_SYNC' 
    END;
```

---

## Next Steps

After successful migration:

1. **Monitor Performance**: Use `EXPLAIN ANALYZE` to verify query improvements
2. **Update Documentation**: Update API docs to reflect new table names
3. **Team Training**: Explain Trinity Pattern benefits to development team
4. **Consider Additional Optimizations**:
   - Add materialized views for analytics
   - Implement database-level caching
   - Set up connection pooling optimization

---

## Related Documentation

- [Table Naming Conventions](./TABLE_NAMING_CONVENTIONS.md) - Complete naming reference
- [View Strategies](./VIEW_STRATEGIES.md) - When to use v_* vs tv_* vs mv_*
- [Trinity Identifiers](./trinity_identifiers.md) - Three-tier ID system
- [Database Level Caching](./DATABASE_LEVEL_CACHING.md) - Performance optimization

---

**Success Criteria:**
- [ ] All tables renamed to `tb_*` prefix
- [ ] API views (`v_*`) created and working
- [ ] Computed views (`tv_*`) created with sync triggers
- [ ] Application code updated and tested
- [ ] Performance improved (queries <1ms for simple lookups)
- [ ] Zero data loss during migration
- [ ] Team trained on Trinity Pattern

**Estimated Time:** 15-30 minutes
**Risk Level:** Low (with proper backup and testing)