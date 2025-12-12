# Phase 5: Remediation - Fix Identified Issues

## Objective

Fix all TRUE violations identified in Phase 4 manual review:
1. Update SQL examples to follow Trinity pattern
2. Fix documentation examples
3. Update Python types to match views
4. Add missing patterns to examples

## Context

Phase 4 identified:
- True violations needing fixes
- Documentation inaccuracies
- Missing patterns in examples
- Legacy code to update

This phase implements fixes systematically.

## Files to Modify/Create

### Modify (Examples)
- `examples/**/*.sql` - Fix Trinity pattern violations
- `examples/**/*.py` - Fix Python type definitions
- `examples/**/README.md` - Update example documentation

### Modify (Documentation)
- `docs/core/concepts-glossary.md` - Fix SQL examples
- `README.md` - Update incorrect examples
- `~/.claude/skills/printoptim-database-patterns.md` - Align with FraiseQL

### Create
- `.phases/verify-examples-compliance/remediation-checklist.md` - Track fixes
- `.phases/verify-examples-compliance/migration-guide.md` - Guide for updating

## Implementation Steps

### Step 1: Prioritize Fixes

Group violations by impact:

**Priority 1: Security Issues (ERROR)**
- VW-003: JSONB exposing pk_* fields
- FK-001: Foreign keys referencing id instead of pk_*

**Priority 2: Breaking Changes (ERROR)**
- TR-001: Missing INTEGER pk_* primary key
- TR-002: Missing UUID id column
- VW-001: Views missing direct id column

**Priority 3: Code Quality (WARNING)**
- HF-002: Variable naming conventions
- MF-002: Missing tv_* sync calls

**Priority 4: Documentation (INFO)**
- Outdated SQL examples in docs
- Missing Trinity pattern in some examples

### Step 2: Fix SQL Examples in Examples

**Example: Update examples/simple_blog/ to Trinity Pattern**

Before (WRONG):
```sql
-- examples/simple_blog/db/schema.sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,  -- ❌ Should use INTEGER GENERATED + UUID
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE NOT NULL
);

CREATE VIEW v_users AS
SELECT
    jsonb_build_object(  -- ❌ Missing direct id column
        'id', id,
        'username', username,
        'email', email
    ) as data
FROM users;
```

After (CORRECT):
```sql
-- examples/simple_blog/db/schema.sql
CREATE TABLE tb_user (  -- ✅ tb_ prefix
    -- ✅ Trinity pattern
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    identifier TEXT UNIQUE NOT NULL,  -- username as identifier

    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,

    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE VIEW v_user AS
SELECT
    id,  -- ✅ Direct column for WHERE filtering
    jsonb_build_object(
        'id', id::text,
        'identifier', identifier,
        'email', email,
        'name', name
        -- ✅ No pk_user in JSONB
    ) as data
FROM tb_user;
```

**Update Python types:**
```python
# examples/simple_blog/app.py
from uuid import UUID

@fraiseql.type(sql_source="v_user", jsonb_column="data")
class User:
    id: UUID          # ✅ Public API
    identifier: str   # ✅ Username as identifier
    email: str
    name: str
    # ✅ No pk_user exposed
```

### Step 3: Fix Documentation Examples

**Fix `docs/core/concepts-glossary.md`**

Lines 298-330 (Projection Tables):

Before:
```sql
-- Sync via trigger (implied)
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB GENERATED ALWAYS AS (...) STORED  -- ❌ Can't do this!
);
```

After:
```sql
-- Sync explicitly in mutations
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,  -- ✅ Regular column
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE FUNCTION fn_sync_tv_user(p_id UUID) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user WHERE id = p_id
    ON CONFLICT (id) DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Mutations call sync explicitly
CREATE FUNCTION fn_create_user(...) RETURNS JSONB AS $$
BEGIN
    INSERT INTO tb_user (...) VALUES (...) RETURNING id INTO v_user_id;
    PERFORM fn_sync_tv_user(v_user_id);  -- ✅ Explicit sync
    RETURN (SELECT data FROM tv_user WHERE id = v_user_id);
END;
$$;
```

**Fix `README.md`**

Lines 520-555 (Mutation example):

Update to match actual FraiseQL mutation pattern with proper JSONB return structure.

### Step 4: Add Missing Patterns to Examples

**Add Trinity Pattern to All Tables:**

```bash
# Find tables without Trinity
for file in examples/*/db/**/*.sql; do
    if grep -q "CREATE TABLE" "$file"; then
        if ! grep -q "pk_\w\+ INTEGER GENERATED" "$file"; then
            echo "$file: Missing INTEGER pk_*"
        fi
        if ! grep -q "id UUID DEFAULT gen_random_uuid" "$file"; then
            echo "$file: Missing UUID id"
        fi
    fi
done
```

For each file missing patterns:
1. Add pk_* INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY
2. Add id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE
3. Consider adding identifier TEXT UNIQUE (if appropriate)
4. Update corresponding views to include id column
5. Update Python types

### Step 5: Create Migration Guide

Document pattern for existing projects:

```markdown
# Trinity Pattern Migration Guide

## For Existing Projects

If you have existing FraiseQL projects without Trinity pattern:

### Step 1: Add Columns to Tables

```sql
-- Add to existing table
ALTER TABLE users
    ADD COLUMN pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ADD COLUMN id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    ADD COLUMN identifier TEXT UNIQUE;

-- Migrate existing primary key
-- (requires careful planning based on foreign keys)
```

### Step 2: Update Views

```sql
-- Old view
CREATE VIEW v_users AS SELECT * FROM users;

-- New view with Trinity
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,  -- Direct column
    jsonb_build_object(
        'id', id::text,
        'identifier', identifier,
        'username', username  -- Keep old fields
    ) as data
FROM users;
```

### Step 3: Update Foreign Keys

```sql
-- Add INTEGER FK (keep old UUID FK temporarily)
ALTER TABLE posts
    ADD COLUMN fk_user INTEGER REFERENCES users(pk_user);

-- Populate from existing data
UPDATE posts SET fk_user = users.pk_user
FROM users WHERE posts.user_id = users.id;

-- Eventually drop old UUID FK
ALTER TABLE posts DROP COLUMN user_id;
```

### Step 4: Update Application Code

```python
# Update type definitions
@fraiseql.type(sql_source="v_user", jsonb_column="data")
class User:
    id: UUID          # Now using UUID id
    identifier: str   # Added
    username: str     # Keep for compatibility
```
```

### Step 6: Verify All Fixes

Re-run verification after fixes:

```bash
# Run verification again
python .phases/verify-examples-compliance/verify.py examples/blog_api/

# Should now show 100% compliance
# Compliance Score: 100.0%
# Errors: 0 | Warnings: 0 | Info: 0
```

**For each fixed example:**
```bash
# Test SQL migrations
createdb test_$(basename "$example")
psql -d test_$(basename "$example") -f "$example/db/schema.sql"

# Run example tests
cd "$example"
pytest -v

# Verify no regressions
```

## Verification Commands

### Check Fixes Applied
```bash
# Verify blog_api now has 0 errors
python verify.py examples/blog_api/ | grep "Errors: 0"

# Check all examples improved
python verify.py examples/*/ > after-remediation.txt
diff before-remediation.txt after-remediation.txt
```

### Test Documentation Examples
```bash
# Extract and test all SQL from docs
./test-doc-examples.sh docs/core/concepts-glossary.md
./test-doc-examples.sh README.md

# All should execute without errors
```

### Regression Testing
```bash
# Run full test suite
cd /home/lionel/code/fraiseql
uv run pytest tests/ -v --tb=short

# No new failures should appear
```

## Expected Output

### Remediation Checklist
```markdown
# Remediation Checklist

## Priority 1: Security Fixes (5 issues)
- [x] examples/old_blog/v_user.sql: Remove pk_user from JSONB
- [x] examples/simple_api/schema.sql: Fix FK to reference pk_*
- [x] examples/demo/views.sql: Add direct id column
- [x] examples/test_app/tables.sql: Add INTEGER pk_* primary key
- [x] examples/minimal/schema.sql: Add UUID id column

## Priority 2: Documentation Updates (3 issues)
- [x] docs/core/concepts-glossary.md:330 - Fix tv_* sync example
- [x] README.md:520-555 - Update mutation example
- [x] docs/core/concepts-glossary.md:176 - Add pk_* column explanation

## Priority 3: Code Quality (8 issues)
- [x] examples/blog_api/functions.sql - Fix variable naming
- [x] examples/ecommerce/mutations.sql - Add explicit sync calls
- [ ] ... (tracked separately)

## Verification Results
- Blog API: 100% compliant ✅
- E-commerce API: 95% compliant (2 warnings acceptable)
- Simple Blog: 100% compliant ✅
- All examples: 97% average compliance ✅
```

### Before/After Metrics
```
Before Remediation:
- Examples with errors: 12/35 (34%)
- Total violations: 47
- Average compliance: 78%

After Remediation:
- Examples with errors: 0/35 (0%)
- Total violations: 3 (all INFO level)
- Average compliance: 99%
```

## Acceptance Criteria

- [ ] All Priority 1 security issues fixed (100%)
- [ ] All Priority 2 breaking issues fixed (100%)
- [ ] Blog API example: 100% compliant (reference)
- [ ] Documentation examples: All executable and correct
- [ ] No test regressions introduced
- [ ] Migration guide created for existing projects
- [ ] Ready for Phase 6 (Documentation Update)

## DO NOT

- ❌ Do NOT break existing tests (fix tests if needed)
- ❌ Do NOT change API contracts without migration path
- ❌ Do NOT rush fixes (test each change)
- ❌ Do NOT skip verification (re-run after each fix)
