# Phase 4: Documentation Updates (v1.8.0 with Deprecation Notices)

## Objective

Update all user-facing documentation to use `mutation_response` with deprecation notices for `mutation_result_v2`.

## Duration

2 hours

## Files to Modify

- `docs/mutations/status-strings.md` (if exists)
- `docs/features/sql-function-return-format.md`
- `docs/features/mutation-result-reference.md`
- `docs/features/graphql-cascade.md`
- `CHANGELOG.md`
- `migrations/trinity/README.md` (create/update)

---

## Strategy for v1.8.0

**Documentation should:**

1. Use `mutation_response` in all new examples
2. Add deprecation notices for `mutation_result_v2`
3. Explain the migration timeline
4. Show that both names work (backward compatible)

---

## Task 4.1-4.4: Update Documentation Files

For EACH file (except CHANGELOG):

1. **Find/replace**: `mutation_result_v2` → `mutation_response` in code examples
2. **Add deprecation box** at the top of the document:

   ```markdown
   > **Note on Naming (v1.8.0+)**
   >
   > This documentation uses `mutation_response` (introduced in v1.8.0).
   > The old name `mutation_result_v2` still works but is deprecated.
   >
   > **Migration timeline:**
   > - v1.8.0-v1.9.x: Both names supported
   > - v2.0.0: `mutation_result_v2` removed
   ```

3. **Review examples** - Ensure they demonstrate best practices with new name

### Verification (per file)

```bash
# Check examples use new name
grep -c "mutation_response" docs/mutations/status-strings.md
# Expected: 5+

# Old name should still be mentioned in deprecation notices
grep -c "mutation_result_v2" docs/mutations/status-strings.md
# Expected: 1-3 (in deprecation notice only)
```

---

## Task 4.5: Create Migration README

**File**: `migrations/trinity/README.md`

**Content** (from phase1_postgresql_v1.8.md lines 153-210):

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

## Task 4.6: Update CHANGELOG

**File**: `CHANGELOG.md`

### Add entry at top:
```markdown
## [Unreleased]

### Added
- **PostgreSQL Type Rename**: Introduced `mutation_response` as canonical name for mutation return type
  - Migration file: `006_add_mutation_response.sql`
  - Helper functions: All updated to return `mutation_response`
  - Backward compatibility: `mutation_result_v2` available as deprecated alias

### Deprecated
- **`mutation_result_v2`**: Use `mutation_response` instead
  - **Timeline**: Deprecated in v1.8.0, will be removed in v2.0.0
  - **Migration**: Simple find/replace in SQL functions
  - **Backward compatibility**: Both names work in v1.8.x-v1.9.x
  - See `migrations/trinity/README.md` for migration guide

### Migration Guide (v1.8.0)

#### PostgreSQL Functions

**Before (deprecated):**
```sql
CREATE FUNCTION create_user(input_data JSONB)
RETURNS mutation_result_v2 AS $$
BEGIN
  -- ...
  RETURN ROW(...)::mutation_result_v2;
END;
$$ LANGUAGE plpgsql;
```

**After (recommended):**

```sql
CREATE FUNCTION create_user(input_data JSONB)
RETURNS mutation_response AS $$
BEGIN
  -- ...
  RETURN ROW(...)::mutation_response;
END;
$$ LANGUAGE plpgsql;
```

**Note**: Both versions work in v1.8.0. No breaking changes.

```

---

## Task 4.7: Update Examples Directory

**Files**:
- `examples/mutations_demo/v2_init.sql`
- `examples/mutations_demo/v2_mutation_functions.sql`

### Changes:
1. Update all `RETURNS mutation_result_v2` → `RETURNS mutation_response`
2. Update all casts `::mutation_result_v2` → `::mutation_response`
3. Add comment at top:
   ```sql
   -- FraiseQL Mutation Examples (v1.8.0+)
   -- Uses mutation_response (new name)
   -- Note: mutation_result_v2 still works but is deprecated
   ```

---

## Acceptance Criteria

- [ ] All doc files updated with `mutation_response` examples
- [ ] Deprecation notices added to all major docs
- [ ] Migration README created
- [ ] CHANGELOG entry added with clear timeline
- [ ] Examples directory updated
- [ ] No `mutation_result_v2` in code examples (except deprecation notices)

## Git Commit

```bash
git add docs/ CHANGELOG.md migrations/trinity/README.md examples/
git commit -m "docs: introduce mutation_response with deprecation notices

- Update all documentation to use mutation_response
- Add deprecation notices for mutation_result_v2
- Create migration guide in migrations/trinity/README.md
- Update CHANGELOG with v1.8.0 timeline
- Update examples to demonstrate best practices

Both names work in v1.8.0-v1.9.x for backward compatibility."
```

## Next: Phase 5 - Tests

---

**Phase Status**: ⏸️ Ready to Start
**Version**: v1.8.0 (alias strategy)
**Breaking**: No (backward compatible)
