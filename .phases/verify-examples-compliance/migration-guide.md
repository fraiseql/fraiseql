# Trinity Pattern Migration Guide

**For Existing FraiseQL Projects**

This guide helps migrate existing projects to the Trinity identifier pattern. The Trinity pattern provides security, performance, and consistency benefits but requires careful migration planning.

## 📋 Migration Overview

### What Changes
- **Tables:** Add `pk_* INTEGER GENERATED` + `id UUID` + `identifier TEXT`
- **Views:** Include `id` column, exclude `pk_*` from JSONB
- **Foreign Keys:** Change to reference `pk_*` (INTEGER) instead of `id` (UUID)
- **Functions:** Update JOINs and parameter types
- **Python Types:** Add `identifier` fields where present in JSONB

### Migration Phases
1. **Assessment** - Analyze current schema
2. **Planning** - Create migration scripts
3. **Execution** - Apply changes with downtime windows
4. **Validation** - Test all functionality
5. **Cleanup** - Remove old columns

---

## 🔍 Phase 1: Assessment

### Analyze Current Schema

```bash
# Find tables without Trinity pattern
for schema_file in db/**/*.sql; do
    echo "=== $schema_file ==="
    # Check for missing pk_*
    if ! grep -q "pk_\w\+ INTEGER GENERATED" "$schema_file"; then
        echo "❌ Missing pk_* INTEGER GENERATED"
    fi
    # Check for missing id UUID
    if ! grep -q "id UUID DEFAULT gen_random_uuid" "$schema_file"; then
        echo "❌ Missing id UUID column"
    fi
    # Check foreign keys
    if grep -q "REFERENCES.*id" "$schema_file"; then
        echo "⚠️  FK references id instead of pk_*"
    fi
done
```

### Identify Dependencies

```sql
-- Find all foreign key relationships
SELECT
    tc.table_name,
    kcu.column_name,
    ccu.table_name AS foreign_table_name,
    ccu.column_name AS foreign_column_name
FROM information_schema.table_constraints AS tc
JOIN information_schema.key_column_usage AS kcu
  ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.constraint_column_usage AS ccu
  ON ccu.constraint_name = tc.constraint_name
WHERE constraint_type = 'FOREIGN KEY';
```

---

## 📝 Phase 2: Planning

### Create Migration Scripts

#### Step 1: Add Trinity Columns

```sql
-- For each existing table, add Trinity columns
-- IMPORTANT: Do this during maintenance window

ALTER TABLE users
    ADD COLUMN pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ADD COLUMN id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    ADD COLUMN identifier TEXT UNIQUE;

-- Populate with existing data
UPDATE users SET
    id = COALESCE(id, gen_random_uuid()),
    identifier = COALESCE(identifier, username);  -- or email, depending on your data

-- Add NOT NULL constraints after population
ALTER TABLE users
    ALTER COLUMN id SET NOT NULL,
    ALTER COLUMN identifier SET NOT NULL;
```

#### Step 2: Update Foreign Keys

```sql
-- Before: UUID foreign keys (slow, exposes internal IDs)
ALTER TABLE posts
    ADD COLUMN fk_user INTEGER REFERENCES users(pk_user);

-- Populate with data
UPDATE posts SET fk_user = users.pk_user
FROM users WHERE posts.user_id = users.id;

-- Verify data integrity
SELECT COUNT(*) FROM posts WHERE fk_user IS NULL;

-- After verification, drop old FK and column
ALTER TABLE posts DROP CONSTRAINT posts_user_id_fkey;
ALTER TABLE posts DROP COLUMN user_id;
ALTER TABLE posts RENAME COLUMN fk_user TO user_id;  -- Optional: keep naming
```

#### Step 3: Update Views

```sql
-- Before: Missing id column, may expose pk_*
CREATE VIEW v_users AS
SELECT
    jsonb_build_object(
        'id', id,
        'username', username
    ) as data
FROM users;

-- After: Include id column, exclude pk_* from JSONB
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,  -- ✅ Direct column for WHERE filtering
    jsonb_build_object(
        'id', id,
        'identifier', identifier,  -- ✅ Human-readable
        'username', username
        -- ✅ pk_user NOT in JSONB (security)
    ) as data
FROM users;
```

#### Step 4: Update Functions

```sql
-- Before: JOIN on id (UUID), parameter types may be wrong
CREATE FUNCTION get_user_posts(user_id UUID)
RETURNS JSONB AS $$
BEGIN
    RETURN (
        SELECT jsonb_agg(data)
        FROM v_posts
        WHERE author_id = user_id  -- Wrong: should be pk_user
    );
END;
$$;

-- After: JOIN on pk_*, correct parameter types
CREATE FUNCTION get_user_posts(p_user_id UUID)
RETURNS JSONB AS $$
DECLARE
    v_user_pk INTEGER;
BEGIN
    -- Resolve UUID to internal pk
    SELECT pk_user INTO v_user_pk
    FROM users WHERE id = p_user_id;

    RETURN (
        SELECT jsonb_agg(data)
        FROM v_posts vp
        WHERE vp.fk_user = v_user_pk  -- ✅ INTEGER JOIN
    );
END;
$$;
```

#### Step 5: Update Python Types

```python
# Before: Missing identifier field
@fraiseql.type(sql_source="v_users")
class User:
    id: UUID
    username: str

# After: Include identifier from JSONB
@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    identifier: str  # ✅ Added
    username: str
```

---

## ⚡ Phase 3: Execution

### Maintenance Window Procedure

```bash
#!/bin/bash
# migration.sh - Execute during maintenance window

set -e  # Exit on any error

echo "Starting Trinity migration..."

# 1. Create backup
pg_dump -Fc mydb > backup_$(date +%Y%m%d_%H%M%S).dump

# 2. Run pre-migration checks
psql -d mydb -f migration_checks.sql

# 3. Add columns (fast)
psql -d mydb -f add_trinity_columns.sql

# 4. Populate data (may take time)
psql -d mydb -f populate_trinity_data.sql

# 5. Update constraints and indexes
psql -d mydb -f update_constraints.sql

# 6. Update views and functions
psql -d mydb -f update_views_functions.sql

# 7. Update application code
# (Deploy new application version)

# 8. Run validation tests
psql -d mydb -f validation_tests.sql

echo "Migration completed successfully!"
```

### Rollback Plan

```sql
-- If migration fails, rollback steps:
-- 1. Restore from backup
-- 2. Or manually remove added columns:
ALTER TABLE users DROP COLUMN pk_user;
ALTER TABLE users DROP COLUMN id;
ALTER TABLE users DROP COLUMN identifier;
-- 3. Restore original foreign keys
```

---

## 🧪 Phase 4: Validation

### Test All Functionality

```sql
-- Test Trinity pattern works
INSERT INTO users (username, email, identifier)
VALUES ('testuser', 'test@example.com', 'test-user')
RETURNING pk_user, id, identifier;

-- Test views work
SELECT id, jsonb_object_keys(data)
FROM v_user
WHERE identifier = 'test-user';

-- Test foreign keys work
INSERT INTO posts (fk_user, title, content)
VALUES (1, 'Test Post', 'Content')
RETURNING id;

-- Test functions work
SELECT get_user_posts(id) FROM users WHERE identifier = 'test-user';
```

### Application Testing

```python
# Test GraphQL queries still work
query = """
{
  user(identifier: "test-user") {
    id
    identifier
    username
  }
}
"""

# Test mutations work
mutation = """
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    id
    title
    author {
      identifier
    }
  }
}
"""
```

---

## 🧹 Phase 5: Cleanup

### Remove Legacy Columns

```sql
-- After full testing and confidence, remove old columns
-- WARNING: This is irreversible!

-- Remove old UUID primary keys
ALTER TABLE users DROP CONSTRAINT users_pkey;
ALTER TABLE users ADD CONSTRAINT users_pkey PRIMARY KEY (pk_user);

-- Remove old UUID columns (if any)
-- ALTER TABLE users DROP COLUMN old_id;

-- Update indexes
DROP INDEX IF EXISTS idx_users_old_id;
CREATE INDEX idx_users_id ON users(id);
CREATE INDEX idx_users_identifier ON users(identifier);
```

### Update Documentation

- Update API documentation to reference `identifier` fields
- Update GraphQL schema documentation
- Update internal developer docs

---

## 🚨 Common Pitfalls

### 1. Foreign Key Migration Order

**Wrong:**
```sql
ALTER TABLE posts ADD COLUMN fk_user INTEGER REFERENCES users(pk_user);
UPDATE posts SET fk_user = users.pk_user FROM users WHERE posts.user_id = users.id;
-- ERROR: users.pk_user doesn't exist yet!
```

**Right:**
```sql
-- First add pk_user to users
ALTER TABLE users ADD COLUMN pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY;

-- Then add FK to posts
ALTER TABLE posts ADD COLUMN fk_user INTEGER REFERENCES users(pk_user);
UPDATE posts SET fk_user = users.pk_user FROM users WHERE posts.user_id = users.id;
```

### 2. JSONB Field Order

**Issue:** Python types must match JSONB field order exactly

```python
# Wrong: Fields in different order than JSONB
@fraiseql.type
class User:
    name: str      # JSONB has id first
    id: UUID       # But Python has id second
```

### 3. Identifier Population

**Challenge:** What to use for identifier values?

```sql
-- Options:
UPDATE users SET identifier = username;     -- If username exists
UPDATE users SET identifier = email;        -- Use email as identifier
UPDATE users SET identifier = 'user-' || id::text;  -- Generate from ID
```

### 4. Performance Impact

**Monitor:** INTEGER FKs are faster, but migration may temporarily slow queries

```sql
-- Monitor query performance during migration
EXPLAIN ANALYZE SELECT * FROM posts JOIN users ON posts.fk_user = users.pk_user;
```

---

## 📊 Success Metrics

### Migration Quality
- **Downtime:** < 30 minutes for most applications
- **Data Loss:** 0% (all migrations are additive first)
- **Query Performance:** 10-50% improvement for JOINs
- **API Compatibility:** 100% (UUID ids remain exposed)

### Post-Migration Benefits
- **Security:** pk_* never exposed in APIs
- **Performance:** INTEGER FKs reduce index sizes by 75%
- **Consistency:** All tables follow same pattern
- **Maintainability:** Predictable schema structure

---

## 🆘 Troubleshooting

### "Duplicate key value violates unique constraint"

**Cause:** Adding `GENERATED ALWAYS AS IDENTITY` to existing table with data

**Fix:**
```sql
-- Set the sequence to start after max existing value
SELECT setval('users_pk_user_seq', (SELECT MAX(pk_user) FROM users));
```

### "Cannot drop column because other objects depend on it"

**Cause:** Views or functions still reference old column names

**Fix:**
```sql
-- Find dependencies
SELECT * FROM information_schema.view_column_usage WHERE table_name = 'users';
-- Update all dependent objects first
```

### "JSONB field order mismatch"

**Cause:** Python type fields don't match JSONB build order

**Fix:**
```sql
-- Check JSONB field order
SELECT jsonb_object_keys(data) FROM v_user LIMIT 1;

-- Match Python type order exactly
@fraiseql.type
class User:
    id: UUID        # Must be first if JSONB has id first
    identifier: str # Must be second if JSONB has identifier second
    # ...
```

---

## 📚 Additional Resources

- [Trinity Pattern Documentation](../docs/core/concepts-glossary.md)
- [Verification Rules](../.phases/verify-examples-compliance/rules.yaml)
- [Golden Patterns](../.phases/verify-examples-compliance/golden-patterns.md)
- [Edge Cases](../.phases/verify-examples-compliance/edge-cases.md)

---

**Need Help?** The Trinity pattern migration requires careful planning. Consider:
- Starting with a non-production copy
- Having a rollback plan ready
- Testing thoroughly before production deployment
- Consulting with your database administrator

The benefits of the Trinity pattern (security, performance, consistency) make the migration worthwhile, but proper execution is critical.
