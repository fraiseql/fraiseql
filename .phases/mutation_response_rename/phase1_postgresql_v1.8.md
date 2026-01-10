# Phase 1: PostgreSQL Migration Files (v1.8.0 with Alias Strategy)

## Objective
Introduce `mutation_response` as the new name while maintaining backward compatibility via alias.

## Duration
2 hours

## Strategy: Dual-Name Support

**Both names work in v1.8.0:**
- `mutation_response` (new, recommended)
- `mutation_result_v2` (old, deprecated, aliased)

**Removal timeline:**
- v1.8.0-v1.9.x: Both names supported
- v2.0.0: Only `mutation_response`

---

## Files to Modify
1. `migrations/trinity/005_add_mutation_result_v2.sql` → `006_add_mutation_response.sql` (NEW)
2. Keep `005_add_mutation_result_v2.sql` (unchanged for existing users)
3. `examples/mutations_demo/v2_init.sql` → Update to use new name
4. `examples/mutations_demo/v2_mutation_functions.sql` → Update to use new name

---

## Task 1.1: Create New Migration with Alias

**File**: `migrations/trinity/006_add_mutation_response.sql` (NEW)

**Create new migration**:
```sql
-- Migration: Add mutation_response type and deprecate mutation_result_v2
-- Description: Introduces clean naming while maintaining backward compatibility
-- Version: 1.8.0
-- Date: 2025-12-04

-- =====================================================
-- MUTATION RESPONSE TYPE (NEW NAME)
-- =====================================================

-- Create the new mutation_response composite type
CREATE TYPE mutation_response AS (
    status          text,
    message         text,
    entity_id       text,
    entity_type     text,
    entity          jsonb,
    updated_fields  text[],
    cascade         jsonb,
    metadata        jsonb
);

-- =====================================================
-- BACKWARD COMPATIBILITY ALIAS
-- =====================================================

-- Create alias for old name (deprecated)
-- This allows existing code to continue working
CREATE TYPE mutation_result_v2 AS (
    status          text,
    message         text,
    entity_id       text,
    entity_type     text,
    entity          jsonb,
    updated_fields  text[],
    cascade         jsonb,
    metadata        jsonb
);

COMMENT ON TYPE mutation_result_v2 IS 'DEPRECATED: Use mutation_response instead. This alias will be removed in v2.0.0.';

-- =====================================================
-- HELPER FUNCTIONS (NEW SIGNATURES)
-- =====================================================

-- All helper functions now return mutation_response
-- (keeping old function names for familiarity)

CREATE OR REPLACE FUNCTION mutation_success(
    message_text text,
    entity_data jsonb,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    entity_id_val := entity_data->>'id';
    RETURN ROW(
        'success'::text,
        message_text,
        entity_id_val,
        entity_type_name,
        entity_data,
        NULL::text[],
        cascade_data,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- [Repeat for all other helper functions...]
-- mutation_created, mutation_updated, mutation_deleted, mutation_noop,
-- mutation_validation_error, mutation_not_found, mutation_conflict, mutation_error

-- [Copy full implementation from 005_add_mutation_result_v2.sql]
```

**Action**:
```bash
cd /home/lionel/code/fraiseql
cp migrations/trinity/005_add_mutation_result_v2.sql migrations/trinity/006_add_mutation_response.sql

# Then manually edit to add the alias and update comments
```

---

## Task 1.2: Update Examples to Use New Name

**Files**:
- `examples/mutations_demo/v2_init.sql`
- `examples/mutations_demo/v2_mutation_functions.sql`

**Strategy**: Update examples to show best practices (use new name)

### Update v2_init.sql

```sql
-- OLD (comment out or remove after adding 006 migration)
-- CREATE TYPE mutation_result_v2 AS (...)

-- NEW (add note)
-- Note: Run migration 006_add_mutation_response.sql to get both types
-- We recommend using mutation_response in new code

-- Then update all RETURNS clauses:
RETURNS mutation_response AS $$  -- was: mutation_result_v2
```

### Update v2_mutation_functions.sql

Same pattern - update all function return types to `mutation_response`.

---

## Task 1.3: Add Migration Documentation

**File**: `migrations/trinity/README.md` (create if doesn't exist)

```markdown
# FraiseQL Database Migrations

## Migration 006: mutation_response Type

**Version**: 1.8.0
**Date**: 2025-12-04

### What Changed

- Introduced `mutation_response` as the new canonical name
- `mutation_result_v2` is now an alias (deprecated)
- All helper functions return `mutation_response`

### Backward Compatibility

**Existing code continues to work** - both type names are supported:

```sql
-- Old code (still works, but deprecated)
CREATE FUNCTION my_func()
RETURNS mutation_result_v2 AS $$
  -- ...
END;

-- New code (recommended)
CREATE FUNCTION my_func()
RETURNS mutation_response AS $$
  -- ...
END;
```

### Migration Path

**No immediate action required.** Migrate at your convenience:

1. **v1.8.x - v1.9.x**: Both names supported
2. **v2.0.0**: `mutation_result_v2` alias removed

### How to Migrate

**Find and replace** in your SQL files:
```bash
# Find uses of old name
grep -r "mutation_result_v2" your_project/

# Replace with new name
sed -i 's/mutation_result_v2/mutation_response/g' your_file.sql
```

### Why the Change?

The "v2" suffix was confusing and implied versioning. The new name is clearer:
- `mutation_response` - What it is (a response from a mutation)
- No version number - Evolves via database migrations, not type name
```

---

## Phase 1 Verification

### Check Files Created

```bash
cd /home/lionel/code/fraiseql

# 1. New migration exists
ls -la migrations/trinity/006_add_mutation_response.sql

# 2. Examples updated
grep "mutation_response" examples/mutations_demo/v2_init.sql | wc -l
# Expected: 10+

# 3. Deprecation comment added
grep -i "DEPRECATED" migrations/trinity/006_add_mutation_response.sql
```

### Test Both Type Names Work

```bash
# If you have psql locally
psql -d test_db << 'EOF'
\i migrations/trinity/006_add_mutation_response.sql

-- Test new name
CREATE FUNCTION test_new()
RETURNS mutation_response AS $$
BEGIN
  RETURN ROW('success', 'test', NULL, NULL, '{}'::jsonb, NULL, NULL, NULL)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Test old name (alias)
CREATE FUNCTION test_old()
RETURNS mutation_result_v2 AS $$
BEGIN
  RETURN ROW('success', 'test', NULL, NULL, '{}'::jsonb, NULL, NULL, NULL)::mutation_result_v2;
END;
$$ LANGUAGE plpgsql;

-- Both should work
SELECT test_new();
SELECT test_old();
EOF
```

---

## Acceptance Criteria

- [ ] New migration `006_add_mutation_response.sql` created
- [ ] Both `mutation_response` and `mutation_result_v2` types exist
- [ ] Deprecation comment on old type
- [ ] All helper functions return `mutation_response`
- [ ] Examples updated to use new name
- [ ] Migration README created
- [ ] Old migration `005_*` unchanged (for existing users)

---

## Git Commit

```bash
git add migrations/trinity/006_add_mutation_response.sql
git add migrations/trinity/README.md
git add examples/mutations_demo/
git commit -m "feat(db): introduce mutation_response with backward compatibility

- Add mutation_response as canonical type name
- Maintain mutation_result_v2 as deprecated alias
- Update examples to use new name
- Both names work in v1.8.0-v1.9.x
- Remove alias in v2.0.0

Migration path documented in migrations/trinity/README.md"
```

---

## Next Steps

✅ **Proceed to Phase 2: Rust Layer Updates**

Location: `.phases/mutation_response_rename/phase2_rust.md`

---

**Phase Status**: ⏸️ Ready to Start
**Estimated Time**: 2 hours
**Breaking**: No (backward compatible)
