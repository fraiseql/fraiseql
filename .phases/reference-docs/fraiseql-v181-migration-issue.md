# FraiseQL v1.8.1 Migration Issue

**Date**: 2025-12-12
**Status**: Tests failing after migration
**Branch**: `feature/fraiseql-mutation-response-alignment`

## Summary

The FraiseQL v1.8.1 migration appears to be partially complete, but tests are failing with schema validation warnings. The mutations have been refactored to use `@fraiseql.success` and `@fraiseql.failure` decorators, but there's a mismatch in auto-injected fields.

## Current State

### ✅ Completed Migration Steps

1. **Error types migrated**: All Error types use `@fraiseql.failure` decorator
2. **Manual `code` fields removed**: No manual `code: int` definitions found (0 occurrences)
3. **Test queries clean**: No `updatedFields` or `id` in Error fragments (0 occurrences)
4. **Modified files**: 47 mutation resolver files changed

### ❌ Test Failures

**Test Results**: 138 failed, 594 passed, 23 skipped (out of 779 tests)

**Primary Issue**: Schema validation warnings appearing in test output:

```
Schema validation warning: Missing expected fields in CreateReservationSuccess: ["status", "errors"]
Schema validation warning: Extra fields in CreateReservationSuccess not in schema: ["id", "updatedFields"]
```

## Problem Analysis

### Expected vs Actual Behavior

According to FraiseQL v1.8.1 changelog:

**Success Types**:
- Should have: `id`, `updatedFields` (auto-injected)
- Should NOT have: `status`, `errors` (only on Error types)

**Error Types**:
- Should have: `status`, `message`, `code` (auto-injected), `errors`
- Should NOT have: `id`, `updatedFields`

### Current Success Type Definition

```python
@fraiseql.success
class CreateMachineSuccess:
    """Success response for machine creation.

    Fields from MutationResultBase:
    - status: str
    - message: str | None
    - errors: list[Error] | None

    Runtime-mapped fields (must be declared for schema):
    - id: UUID (populated from database entity_id at runtime)
    - updatedFields: list[str] (populated from database updated_fields at runtime)
    """

    # These fields must be declared for GraphQL schema, even though values come from DB

    machine: Machine
    cascade: Cascade | None = None
```

**Issue**: The docstring mentions `status`, `errors`, `id`, and `updatedFields`, but:
- None of these are actually defined as class fields
- Tests expect `id` and `updatedFields` to be in schema
- Tests don't expect `status` and `errors` on Success types

## Possible Root Causes

1. **Missing field definitions**: Success types may need explicit `id` and `updatedFields` field definitions
2. **FraiseQL version mismatch**: PrintOptim may not be using the correct FraiseQL v1.8.1
3. **Migration incomplete**: Additional changes needed beyond removing `code` from Error types
4. **Test expectations outdated**: Tests may be checking for old schema structure

## Files Affected

### Modified Mutation Resolvers (47 files)
```
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/agreement/contract/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/agreement/order/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/agreement/price/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/geo/location/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/geo/public_address/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/mat/machine/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/mat/machine_item/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/network/*/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/dim/org/organizational_unit/gql_*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/scd/allocation/*.py
src/printoptim_backend/entrypoints/api/resolvers/mutation/scd/reservation/*.py
```

## Next Steps

1. **Verify FraiseQL version**: Check which FraiseQL commit is actually being used
   ```bash
   cd /home/lionel/code/fraiseql
   git log --oneline -1
   ```

2. **Check FraiseQL field injection**: Review how v1.8.1 auto-injects fields on Success types
   - Read: `/home/lionel/code/fraiseql/src/fraiseql/decorator.py`
   - Look for: `@fraiseql.success` decorator implementation

3. **Review migration guide accuracy**: The guide at `.phases/fraiseql-v1.8.1-migration-guide.md` may be incomplete

4. **Check if Success types need field definitions**: May need to explicitly add:
   ```python
   @fraiseql.success
   class CreateMachineSuccess:
       # Explicit field definitions needed?
       id: uuid.UUID
       updatedFields: list[str]

       machine: Machine
       cascade: Cascade | None = None
   ```

5. **Investigate schema validation source**: Find where "Schema validation warning" messages come from
   - Could be in FraiseQL itself
   - Could be in test utilities
   - Search: `rg "Schema validation" src/ tests/`

## Rollback Instructions

If needed, rollback the migration:

```bash
git diff > /tmp/fraiseql-v181-changes.patch
git checkout src/printoptim_backend/entrypoints/api/resolvers/mutation/
make test  # Verify old tests pass
```

## References

- Migration guide: `.phases/fraiseql-v1.8.1-migration-guide.md`
- FraiseQL v1.8.1 CHANGELOG: `/home/lionel/code/fraiseql/CHANGELOG.md`
- FraiseQL commit: `06939d09` (v1.8.1 release)
- Related files in `.phases/TODO/04_PLANNING/`:
  - `FRAISEQL_AUTO_POPULATE_MIGRATION.md`
  - `FRAISEQL_AUTO_POPULATE_MIGRATION_COMPLETE.md`
  - `FRAISEQL_LOCAL_V180_RESULTS.md`

## Investigation Commands

```bash
# Check current FraiseQL version
cd /home/lionel/code/fraiseql && git log --oneline -1

# Find schema validation warning source
rg "Schema validation warning" src/ tests/ /home/lionel/code/fraiseql/

# Check if id/updatedFields are in GraphQL schema
# (run server and introspection query)

# Compare with working FraiseQL tests
cd /home/lionel/code/fraiseql
pytest tests/mutations/test_canary.py -xvs
```

## Questions to Answer

1. Does FraiseQL v1.8.1 auto-inject `id` and `updatedFields` on Success types, or do they need explicit definitions?
2. Where is the "Schema validation warning" message coming from?
3. Are the test expectations correct, or do tests need updating?
4. Is there a working example of FraiseQL v1.8.1 Success type in the FraiseQL test suite?
