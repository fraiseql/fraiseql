# Work Package: Create Trinity Pattern Migration Guide

**Package ID:** WP-003
**Assignee Role:** Technical Writer - Core Docs (TW-CORE)
**Priority:** P0 - Critical
**Estimated Hours:** 6 hours
**Dependencies:** WP-002 (Database docs fixed)

---

## ⚠️ Execution Requirement

**❌ DO NOT USE LOCAL 8B MODELS FOR THIS WORK PACKAGE**

**This work package REQUIRES Claude (Sonnet 4.5 or better)**

**Why this cannot be delegated to local models:**
- **Content creation** (8-12 pages of coherent documentation)
- **Architectural reasoning** (migration patterns, edge cases, trade-offs)
- **Troubleshooting expertise** (rollback plans, common mistakes)
- **Coherent examples** (examples must work together across multiple sections)
- **Edge case analysis** (foreign keys, triggers, materialized views, RLS)

**What happens if you try local models:**
- ❌ Hallucinated migration steps (dangerous for database operations)
- ❌ Inconsistent examples (section 1 uses different pattern than section 5)
- ❌ Missing critical edge cases (FKs break, data loss)
- ❌ Poor troubleshooting advice (generic, not specific to trinity pattern)

**Estimated cost with Claude:** ~$2-3 (input/output tokens for 8-12 page doc)
**Time with Claude:** 6 hours (as estimated)
**Quality with Claude:** 4.5/5 or higher

**Alternative:** None. This requires deep reasoning and architectural expertise.

---

## Objective

Create a comprehensive migration guide that helps users transition from simple table naming (`users`, `posts`) to the trinity pattern (`tb_user`, `v_user`, `tv_user_with_posts`).

---

## Scope

### Included
- Step-by-step migration instructions (SQL + Python)
- Common edge cases and solutions
- Testing verification steps
- Rollback plan
- Time estimates for migration

### Excluded
- Generic database migration topics (covered elsewhere)
- Framework-specific migrations (Strawberry, Graphene - covered in WP-migration guides)
- Performance tuning (covered in performance docs)

---

## Deliverables

- [ ] New File: `docs/database/migrations.md` - Complete migration guide (8-12 pages)
- [ ] Example migration script (SQL)
- [ ] Testing checklist
- [ ] Troubleshooting section

---

## Acceptance Criteria

### Must Pass All:

- [ ] **Copy-paste ready migration steps** (tested by ENG-QA)
- [ ] **Covers all 3 migration scenarios:**
  1. Single table migration (users → tb_user + v_user)
  2. Multi-table migration (users + posts → tb_user + tb_post + views)
  3. Existing views migration (users_view → v_user)
- [ ] **Testing section:** How to verify nothing broke
- [ ] **Rollback plan:** If migration fails, how to revert
- [ ] **Time estimates:** 15-30 minutes for simple, 1-2 hours for complex
- [ ] **Edge cases covered:**
  - Existing foreign keys
  - Triggers on tables
  - Existing materialized views
  - Application code updates (Python/FastAPI)
- [ ] **Links to related docs:** trinity-pattern.md, naming-conventions.md
- [ ] **Follows style guide** (active voice, time estimates, next steps)

---

## Resources

### Source Code to Reference
- `/home/lionel/code/fraiseql/examples/blog_simple/db/setup.sql` - Target pattern
- `/home/lionel/code/fraiseql/tests/integration/` - Examples of correct usage

### Related Docs
- `docs/core/trinity-pattern.md` (WP-001) - Explains WHY trinity pattern
- `docs/database/TABLE_NAMING_CONVENTIONS.md` (WP-002) - Authoritative naming guide
- `docs/database/trinity-identifiers.md` (WP-002) - Trinity deep dive

### Related Work Packages
- **Depends on:** WP-002 (database docs must be fixed first)
- **Related:** WP-001 (trinity-pattern.md)

---

## Implementation Steps

### Step 1: Research Migration Patterns (1 hour)

**Analyze common migration scenarios:**

1. **Simple table** (no views, no FKs):
   ```sql
   -- BEFORE
   CREATE TABLE users (id UUID PRIMARY KEY, name TEXT);

   -- AFTER
   CREATE TABLE tb_user (id UUID PRIMARY KEY, name TEXT);
   CREATE VIEW v_user AS SELECT * FROM tb_user;
   ```

2. **Table with foreign keys:**
   ```sql
   -- BEFORE
   CREATE TABLE posts (id UUID PRIMARY KEY, user_id UUID REFERENCES users(id));

   -- AFTER: Foreign keys reference base tables
   CREATE TABLE tb_post (id UUID PRIMARY KEY, user_id UUID REFERENCES tb_user(id));
   ```

3. **Existing view:**
   ```sql
   -- BEFORE
   CREATE VIEW users_view AS SELECT * FROM users;

   -- AFTER: Rename to v_ pattern
   CREATE VIEW v_user AS SELECT * FROM tb_user;
   ```

**Output:** Migration pattern catalog

---

### Step 2: Write Migration Guide Structure (4 hours)

**File:** `docs/database/migrations.md`

**Recommended structure:**

```markdown
# Migrating from Simple Tables to Trinity Pattern

**Time to complete:** 15-30 minutes (simple), 1-2 hours (complex)
**Prerequisites:**
- PostgreSQL 14+
- FraiseQL v1.8.0+
- Backup of database (before migration)

## When to Migrate

Migrate to trinity pattern when:
- Moving prototype to production
- Team project (multiple developers)
- Need to add filtering/security (views provide this)
- Planning computed views (aggregations, joins)

## Before You Start

### 1. Take a Backup

```bash
pg_dump your_database > backup_before_trinity_migration.sql
```

### 2. Identify Tables to Migrate

```sql
-- List all tables without tb_ prefix
SELECT tablename
FROM pg_tables
WHERE schemaname = 'public'
  AND tablename NOT LIKE 'tb_%'
  AND tablename NOT LIKE 'pg_%';
```

### 3. Check Dependencies

```sql
-- Check foreign keys
SELECT
    tc.table_name,
    kcu.column_name,
    ccu.table_name AS foreign_table
FROM information_schema.table_constraints AS tc
JOIN information_schema.key_column_usage AS kcu
  ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.constraint_column_usage AS ccu
  ON ccu.constraint_name = tc.constraint_name
WHERE tc.constraint_type = 'FOREIGN KEY';
```

---

## Migration Steps

### Step 1: Rename Base Tables

**For single table (e.g., users):**

```sql
-- Rename table to tb_ prefix
ALTER TABLE users RENAME TO tb_user;
```

**For multiple tables (preserve FK relationships):**

```sql
-- Order matters! Start with tables that have no dependencies
ALTER TABLE users RENAME TO tb_user;
ALTER TABLE posts RENAME TO tb_post;
ALTER TABLE comments RENAME TO tb_comment;

-- Foreign keys are automatically updated by PostgreSQL
```

**Verification:**
```sql
-- Check table exists
\dt tb_user

-- Check foreign keys still work
SELECT * FROM pg_constraint WHERE conname LIKE '%tb_%';
```

---

### Step 2: Create Views

**Simple view (v_):**

```sql
CREATE VIEW v_user AS
SELECT * FROM tb_user;
```

**View with filtering (soft deletes):**

```sql
CREATE VIEW v_user AS
SELECT
    id,
    name,
    email,
    created_at
FROM tb_user
WHERE deleted_at IS NULL;  -- Hide soft-deleted users
```

**View with security (hide sensitive fields):**

```sql
CREATE VIEW v_user_public AS
SELECT
    id,
    name,
    -- email excluded (sensitive)
    created_at
FROM tb_user
WHERE deleted_at IS NULL;
```

**Verification:**
```sql
-- Check view exists
\dv v_user

-- Test query
SELECT * FROM v_user LIMIT 5;
```

---

### Step 3: Create Computed Views (Optional)

**Computed view with aggregations (tv_):**

```sql
CREATE VIEW tv_user_with_stats AS
SELECT
    u.id,
    u.name,
    u.email,
    COUNT(p.id) as post_count,
    MAX(p.created_at) as last_post_at,
    AVG(p.view_count) as avg_post_views
FROM tb_user u
LEFT JOIN tb_post p ON p.user_id = u.id
GROUP BY u.id, u.name, u.email;
```

**When to use computed views:**
- Expensive joins (avoid N+1 queries)
- Aggregations (count, sum, average)
- Denormalized data (for GraphQL performance)

**Verification:**
```sql
-- Check computed view
SELECT * FROM tv_user_with_stats LIMIT 5;

-- Performance check
EXPLAIN ANALYZE SELECT * FROM tv_user_with_stats;
```

---

### Step 4: Update Application Code

**Update FraiseQL schema (Python):**

**BEFORE:**
```python
from fraiseql import GraphQLType

class User(GraphQLType):
    __tablename__ = "users"  # OLD
```

**AFTER:**
```python
from fraiseql import GraphQLType

class User(GraphQLType):
    __tablename__ = "v_user"  # NEW: Expose view, not base table
```

**Important:** Expose `v_user` (view), NOT `tb_user` (base table)

**Update raw SQL queries:**

**BEFORE:**
```python
result = await db.execute("SELECT * FROM users")
```

**AFTER:**
```python
# For queries: Use view
result = await db.execute("SELECT * FROM v_user")

# For inserts/updates: Use base table
result = await db.execute("INSERT INTO tb_user (name) VALUES ($1)", name)
```

**Rule of thumb:**
- **Queries (SELECT):** Use views (`v_*` or `tv_*`)
- **Mutations (INSERT/UPDATE/DELETE):** Use base tables (`tb_*`)

---

### Step 5: Test Migration

**Testing checklist:**

- [ ] **Schema validation:**
  ```sql
  -- Check all base tables exist
  \dt tb_*

  -- Check all views exist
  \dv v_*
  \dv tv_*

  -- Check foreign keys intact
  SELECT * FROM information_schema.table_constraints
  WHERE constraint_type = 'FOREIGN KEY';
  ```

- [ ] **Data integrity:**
  ```sql
  -- Row counts should match
  SELECT COUNT(*) FROM tb_user;  -- Should equal old "users" count
  SELECT COUNT(*) FROM v_user;   -- Should equal tb_user (unless filtered)
  ```

- [ ] **Application tests:**
  ```bash
  # Run your test suite
  pytest tests/

  # Check GraphQL queries work
  curl -X POST http://localhost:8000/graphql \
    -H "Content-Type: application/json" \
    -d '{"query": "{ users { id name } }"}'
  ```

- [ ] **Performance check:**
  ```sql
  EXPLAIN ANALYZE SELECT * FROM v_user;
  EXPLAIN ANALYZE SELECT * FROM tv_user_with_stats;
  ```

**Expected:** No errors, performance similar or better

---

## Edge Cases & Solutions

### Case 1: Existing Triggers on Tables

**Problem:** Triggers reference old table name

**Solution:**
```sql
-- Triggers are automatically updated with ALTER TABLE RENAME
-- But verify:
\dy tb_user  -- Show triggers on tb_user

-- If trigger missing, recreate:
CREATE TRIGGER update_modified_time
BEFORE UPDATE ON tb_user
FOR EACH ROW EXECUTE FUNCTION update_modified_column();
```

---

### Case 2: Materialized Views

**Problem:** Materialized view references old table

**Solution:**
```sql
-- Drop old materialized view
DROP MATERIALIZED VIEW IF EXISTS users_cached;

-- Recreate with new naming
CREATE MATERIALIZED VIEW tv_user_cached AS
SELECT * FROM v_user;

-- Create index
CREATE UNIQUE INDEX ON tv_user_cached (id);
```

---

### Case 3: Foreign Keys to Renamed Tables

**Problem:** Other tables reference old table name

**Good news:** PostgreSQL automatically updates foreign key constraints when using `ALTER TABLE RENAME`

**Verification:**
```sql
-- Check FK still points to tb_user
SELECT
    tc.constraint_name,
    tc.table_name,
    kcu.column_name,
    ccu.table_name AS foreign_table
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu
  ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.constraint_column_usage ccu
  ON ccu.constraint_name = tc.constraint_name
WHERE tc.constraint_type = 'FOREIGN KEY'
  AND ccu.table_name = 'tb_user';
```

---

### Case 4: Application Code Using Table Names

**Problem:** Direct SQL queries in code use old names

**Solution:** Search and replace in codebase

```bash
# Find all references to old table names
grep -r "FROM users" src/
grep -r "INSERT INTO users" src/
grep -r "UPDATE users" src/

# Update to:
# Queries: FROM v_user
# Mutations: INSERT INTO tb_user
```

---

## Rollback Plan

If migration fails, rollback:

### 1. Restore from Backup

```bash
# Drop current database
dropdb your_database

# Restore from backup
createdb your_database
psql your_database < backup_before_trinity_migration.sql
```

### 2. Partial Rollback (if only views created)

```sql
-- Drop views (keeps base tables)
DROP VIEW IF EXISTS v_user;
DROP VIEW IF EXISTS tv_user_with_stats;

-- Rename tables back
ALTER TABLE tb_user RENAME TO users;
```

---

## Common Mistakes

### Mistake 1: Exposing Base Tables to GraphQL

**WRONG:**
```python
class User(GraphQLType):
    __tablename__ = "tb_user"  # DON'T expose base table
```

**RIGHT:**
```python
class User(GraphQLType):
    __tablename__ = "v_user"  # Expose view
```

**Why:** Base tables should be internal. Views provide abstraction layer.

---

### Mistake 2: Querying Base Tables Directly

**WRONG:**
```sql
SELECT * FROM tb_user;  -- Avoid in application code
```

**RIGHT:**
```sql
SELECT * FROM v_user;  -- Use view for queries
```

**Exception:** Admin tools, migrations, debugging can query base tables.

---

### Mistake 3: Inserting into Views

**WRONG:**
```sql
INSERT INTO v_user (name) VALUES ('Alice');  -- Won't work (views are read-only by default)
```

**RIGHT:**
```sql
INSERT INTO tb_user (name) VALUES ('Alice');  -- Insert into base table
```

**Note:** Views can be made updatable with triggers, but not recommended for beginners.

---

## Next Steps

After successful migration:

- [ ] Read [Trinity Pattern Deep Dive](trinity-identifiers.md) - Understand advanced patterns
- [ ] Read [View Strategies](VIEW_STRATEGIES.md) - When to use v_ vs tv_
- [ ] Read [Database Caching](DATABASE_LEVEL_CACHING.md) - Materialized views
- [ ] Update team documentation - Document your schema conventions
- [ ] Consider CI/CD updates - Ensure migrations run in deployment pipeline

---

## FAQ

**Q: Do I need to migrate all tables at once?**
A: No. Migrate incrementally. Start with one table, test, then continue.

**Q: Can I have both simple tables and trinity tables in same database?**
A: Yes, but not recommended. Causes confusion. Migrate all or none.

**Q: What if I don't need views (no filtering)?**
A: Still create views. Future-proofs your schema. Adding filtering later is easier.

**Q: Performance impact of views?**
A: Negligible for simple views. PostgreSQL optimizes them away. Computed views (tv_) may be slower (use materialized views for caching).

**Q: Can I use this with other ORMs (SQLAlchemy, etc.)?**
A: Yes. Trinity pattern is database-level, works with any ORM.

---

**Time Estimate:**
- Simple migration (1-2 tables): 15-30 minutes
- Medium migration (3-10 tables): 1-2 hours
- Complex migration (10+ tables, many FKs): 2-4 hours
```

---

### Step 3: Add Example Migration Script (1 hour)

**Create example SQL script:**

```sql
-- example-migration.sql
-- Migrates simple blog schema from users/posts to tb_user/tb_post

BEGIN;

-- Step 1: Rename base tables
ALTER TABLE users RENAME TO tb_user;
ALTER TABLE posts RENAME TO tb_post;
ALTER TABLE comments RENAME TO tb_comment;

-- Step 2: Create views
CREATE VIEW v_user AS
SELECT id, name, email, created_at
FROM tb_user
WHERE deleted_at IS NULL;

CREATE VIEW v_post AS
SELECT id, user_id, title, content, created_at
FROM tb_post
WHERE deleted_at IS NULL;

CREATE VIEW v_comment AS
SELECT id, post_id, user_id, content, created_at
FROM tb_comment
WHERE deleted_at IS NULL;

-- Step 3: Create computed views
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

CREATE VIEW tv_post_with_stats AS
SELECT
    p.id,
    p.title,
    p.user_id,
    u.name as author_name,
    COUNT(c.id) as comment_count,
    MAX(c.created_at) as last_comment_at
FROM tb_post p
JOIN tb_user u ON u.id = p.user_id
LEFT JOIN tb_comment c ON c.post_id = p.id
GROUP BY p.id, p.title, p.user_id, u.name;

COMMIT;

-- Verification
SELECT 'tb_user rows' as check, COUNT(*) as count FROM tb_user
UNION ALL
SELECT 'v_user rows', COUNT(*) FROM v_user
UNION ALL
SELECT 'tv_user_with_stats rows', COUNT(*) FROM tv_user_with_stats;
```

**Include in migrations.md as downloadable example**

---

## Success Metrics

### For This Work Package

- **Guide completeness:** 8-12 pages, all scenarios covered
- **Testing:** ENG-QA successfully migrates test database in <30 min
- **Clarity:** Junior dev persona can follow without errors
- **Edge cases:** Covers FKs, triggers, materialized views, app code
- **Quality score:** 4.5/5 or higher

### Reader Impact

**Before:** Users don't know how to migrate:
- Afraid to rename tables (will it break FKs?)
- Unsure how to update application code
- No rollback plan (risky)

**After:** Users confidently migrate:
- Step-by-step instructions work
- Edge cases covered
- Rollback plan if needed
- Time estimates accurate

---

## Timeline

**Total Hours:** 6 hours

| Task | Hours | Completion |
|------|-------|------------|
| Research migration patterns | 1 | Day 1 AM |
| Write migration guide | 4 | Day 1 PM - Day 2 AM |
| Add example script | 1 | Day 2 PM |

**Deadline:** End of Day 2 (Week 2)
**Dependency:** WP-002 must be complete (database docs fixed)

---

**End of Work Package WP-003**
