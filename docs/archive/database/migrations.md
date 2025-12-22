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

## Quick Start (5 minutes)

**Option 1: Production-Grade Tool (Recommended)**

For production environments, use [**Confiture**](https://github.com/fraiseql/confiture) - FraiseQL's official migration tool:

```bash
# Install Confiture
pip install fraiseql-confiture

# Initialize project
confiture init

# Generate migration
confiture migrate generate --name "trinity-migration"

# Apply migration
confiture migrate up
```

**Key features:**
- 4 migration strategies (including zero-downtime)
- Build databases from DDL in <1 second
- Production data sync with PII anonymization
- Rust-powered performance

**Option 2: Manual Migration (Quick Start)**

For development or simple cases, use the example script:

```bash
# Backup first!
pg_dump your_database > backup.sql

# Run migration script
psql -d your_database -f docs/database/example-migration.sql
```

The script handles:
- Table renaming (`users` → `tb_user`)
- View creation (`v_user`, `v_post`)
- Computed view creation (`tv_user_with_stats`)
- Verification queries

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

## Migration Steps

### Step 1: Assessment (2 minutes)

**Inventory your tables:**
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

**Check foreign key relationships:**
```sql
-- Map relationships between tables
SELECT
    tc.table_name,
    ccu.table_name AS foreign_table_name,
    ccu.column_name AS foreign_column_name
FROM information_schema.table_constraints AS tc
JOIN information_schema.constraint_column_usage AS ccu
  ON ccu.constraint_name = tc.constraint_name
WHERE tc.constraint_type = 'FOREIGN KEY'
  AND tc.table_schema = 'public';
```

### Step 2: Database Migration (5 minutes)

**Option A: Use Example Script (Recommended)**
```bash
# Run the pre-built migration
psql -d your_database -f docs/database/example-migration.sql
```

**Option B: Manual Migration**
```sql
-- Rename tables with tb_ prefix
ALTER TABLE users RENAME TO tb_user;
ALTER TABLE posts RENAME TO tb_post;
ALTER TABLE comments RENAME TO tb_comment;

-- Create views for GraphQL API
CREATE VIEW v_user AS
SELECT id, name, email, created_at
FROM tb_user
WHERE deleted_at IS NULL;

CREATE VIEW v_post AS
SELECT id, user_id, title, content, created_at
FROM tb_post
WHERE deleted_at IS NULL;

-- Create computed views with pre-computed data
CREATE VIEW tv_user_with_stats AS
SELECT
    u.id,
    u.name,
    u.email,
    COUNT(DISTINCT p.id) as post_count,
    COUNT(DISTINCT c.id) as comment_count,
    MAX(p.created_at) as last_post_at
FROM tb_user u
LEFT JOIN tb_post p ON p.user_id = u.id
LEFT JOIN tb_comment c ON c.user_id = u.id
GROUP BY u.id, u.name, u.email;
```

### Step 3: Application Updates (5 minutes)

**Update FraiseQL types:**
```python
# Before (simple)
@fraiseql.type(sql_source="users")
class User:
    id: UUID
    email: str
    name: str

# After (trinity)
@fraiseql.type(sql_source="tv_user_with_stats")
class UserWithStats:
    id: UUID
    email: str
    name: str
    post_count: int
    comment_count: int

@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
```

**Update queries:**
```python
# Before
@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("users", id=id)

# After
@fraiseql.query
async def user_with_stats(info, id: UUID) -> UserWithStats:
    db = info.context["db"]
    return await db.find_one("tv_user_with_stats", id=id)
```

### Step 4: Testing (3 minutes)

**Verify data integrity:**
```sql
-- Check all data migrated correctly
SELECT
    'tb_user rows' as check, COUNT(*) as count FROM tb_user
UNION ALL
SELECT 'v_user rows', COUNT(*) FROM v_user
UNION ALL
SELECT 'tv_user_with_stats rows', COUNT(*) FROM tv_user_with_stats;
```

**Test performance:**
```sql
-- Compare query performance
EXPLAIN ANALYZE SELECT * FROM users WHERE id = $1;
-- Expected: 5-10ms (table scan)

EXPLAIN ANALYZE SELECT * FROM tv_user_with_stats WHERE id = $1;
-- Expected: 0.05-0.5ms (indexed lookup)
```

---

## Production Migration with Confiture

**For production environments, use [Confiture](https://github.com/fraiseql/confiture) - FraiseQL's official migration tool.**

### Migration Strategies

Confiture offers 4 migration strategies:

1. **Build from DDL** - Create fresh databases in <1 second
2. **Incremental Migrations** - Standard `confiture migrate up`
3. **Production Data Sync** - `confiture sync --from production --anonymize users.email`
4. **Zero-Downtime** - `confiture migrate schema-to-schema --strategy fdw`

### Basic Workflow

```bash
# 1. Install
pip install fraiseql-confiture

# 2. Initialize project
confiture init

# 3. Edit DDL files in db/schema/
cat > db/schema/users.sql <<EOF
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE VIEW v_user AS
SELECT id, name, email, created_at
FROM tb_user
WHERE deleted_at IS NULL;
EOF

# 4. Generate migration
confiture migrate generate --name "add_trinity_pattern"

# 5. Apply migration
confiture migrate up
```

### Zero-Downtime Migrations

For production migrations with minimal downtime (0-5 seconds):

```bash
confiture migrate schema-to-schema --strategy fdw
```

This uses **Foreign Data Wrapper (FDW)** technology. For detailed steps, see [Confiture's Zero-Downtime Guide](https://github.com/fraiseql/confiture/blob/main/docs/guides/medium-4-schema-to-schema.md).

### Learn More

- [Confiture Documentation](https://github.com/fraiseql/confiture#readme)
- [Migration Strategies Guide](https://github.com/fraiseql/confiture/tree/main/docs/guides)

---

## Common Issues and Solutions

### Foreign Key Constraints

**Problem:** Foreign keys reference old table names
```sql
-- Before migration
ALTER TABLE posts ADD CONSTRAINT fk_user
FOREIGN KEY (user_id) REFERENCES users(id);
```

**Solution:** Update foreign keys
```sql
-- After migration
ALTER TABLE tb_post ADD CONSTRAINT fk_user
FOREIGN KEY (user_id) REFERENCES tb_user(id);
```

### Existing Views Break

**Problem:** Views reference renamed tables
```sql
-- This view breaks after rename
CREATE VIEW user_summary AS
SELECT COUNT(*) FROM users;
```

**Solution:** Update view definitions
```sql
-- Update to use new table name
CREATE OR REPLACE VIEW user_summary AS
SELECT COUNT(*) FROM tb_user;
```

### Application Code References

**Problem:** Hard-coded SQL references old names
```python
# This breaks after migration
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

### Edge Case 5: Triggers on Tables

**Problem:** Existing triggers reference old table names
```sql
-- Trigger breaks after rename
CREATE TRIGGER trg_user_audit
AFTER INSERT OR UPDATE ON users
FOR EACH ROW EXECUTE FUNCTION log_user_change();
```

**Solution:** Update triggers to use new table names
```sql
-- Update trigger for new table name
CREATE OR REPLACE TRIGGER trg_user_audit
AFTER INSERT OR UPDATE ON tb_user
FOR EACH ROW EXECUTE FUNCTION log_user_change();

-- Also update trigger function if it references table names
CREATE OR REPLACE FUNCTION log_user_change()
RETURNS TRIGGER AS $$
BEGIN
    -- Update any hard-coded table references in function
    INSERT INTO audit_log (table_name, action, data)
    VALUES ('tb_user', TG_OP, row_to_json(NEW));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

**Solution:** Use FraiseQL repository pattern
```python
# Use FraiseQL abstraction
user = await db.find_one("v_user", id=user_id)
```

---

## Rollback Plan

**If migration fails, rollback immediately:**

```sql
-- Reverse table renames
ALTER TABLE tb_user RENAME TO users;
ALTER TABLE tb_post RENAME TO posts;
ALTER TABLE tb_comment RENAME TO comments;

-- Drop new objects
DROP VIEW IF EXISTS v_user;
DROP VIEW IF EXISTS v_post;
DROP VIEW IF EXISTS tv_user_with_stats;
```

**Application rollback:**
```python
# Revert type definitions
@fraiseql.type(sql_source="users")
class User:
    # ... original definition

# Revert queries
@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("users", id=id)
```

---

## Migration Checklist

### Pre-Migration
- [ ] **Backup database** (`pg_dump your_db > backup.sql`)
- [ ] **Test on staging** (never migrate production directly)
- [ ] **Document current schema** (`pg_dump --schema-only > schema_before.sql`)

### Migration
- [ ] **Run example script** or manual migration steps
- [ ] **Verify row counts** match between old and new tables
- [ ] **Test sample queries** work correctly
- [ ] **Check performance** improvement

### Post-Migration
- [ ] **Update application code** (type definitions, queries)
- [ ] **Run test suite** (all tests must pass)
- [ ] **Monitor for errors** (check logs for 1 hour)
- [ ] **Update documentation** (API docs, READMEs)

---

## Performance Results

**Expected improvements after migration:**

| Operation | Before (simple) | After (trinity) | Improvement |
|-----------|------------------|------------------|-------------|
| User lookup | 5-10ms | 0.05-0.5ms | 10-100x faster |
| User with posts | 15-25ms | 0.1-0.8ms | 25-250x faster |
| User statistics | 50-100ms | 0.2-1.0ms | 50-500x faster |

---

## Next Steps

After successful migration:

1. **Monitor Performance**: Use `EXPLAIN ANALYZE` to verify improvements
2. **Update Documentation**: Update API docs to reflect new table names
3. **Team Training**: Explain Trinity Pattern benefits to developers
4. **Consider Additional Optimizations**:
   - Add materialized views for analytics
   - Implement database-level caching
   - Set up connection pooling

---

## Related Documentation

- [**Confiture**](https://github.com/fraiseql/confiture) - Official FraiseQL migration tool (production-ready)
- [Table Naming Conventions](./table-naming-conventions.md) - Complete naming reference
- [View Strategies](./view-strategies.md) - When to use v_* vs tv_* vs mv_*
- [Trinity Identifiers](./trinity-identifiers.md) - Three-tier ID system
- [Example Migration Script](./example-migration.sql) - Ready-to-use SQL script

---

**Success Criteria:**
- [ ] All tables renamed to `tb_*` prefix
- [ ] API views (`v_*`) created and working
- [ ] Computed views (`tv_*`) created and returning data
- [ ] Application code updated and tested
- [ ] Performance improved (queries <1ms for simple lookups)
- [ ] Zero data loss during migration

**Estimated Time:** 15-30 minutes
**Risk Level:** Low (with proper backup and testing)
