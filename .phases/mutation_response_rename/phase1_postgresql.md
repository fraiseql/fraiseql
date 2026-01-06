# Phase 1: PostgreSQL Migration Files

## Objective
Rename the PostgreSQL composite type and all helper functions from `mutation_result_v2` to `mutation_response`.

## Duration
2 hours

## Files to Modify
1. `migrations/trinity/005_add_mutation_result_v2.sql` → rename to `005_add_mutation_response.sql`
2. `examples/mutations_demo/v2_init.sql`
3. `examples/mutations_demo/v2_mutation_functions.sql`

---

## Task 1.1: Rename Main Migration File

### Step 1: Rename the File
```bash
cd /home/lionel/code/fraiseql
git mv migrations/trinity/005_add_mutation_result_v2.sql \
        migrations/trinity/005_add_mutation_response.sql
```

**Verification**:
```bash
ls -la migrations/trinity/005_add_mutation_response.sql
! ls -la migrations/trinity/005_add_mutation_result_v2.sql  # Should not exist
```

### Step 2: Update Migration Header

**File**: `migrations/trinity/005_add_mutation_response.sql`

**Change** (lines 1-4):
```sql
-- OLD
-- Migration: Add mutation_result_v2 type and helper functions

-- NEW
-- Migration: Add mutation_response type and helper functions
```

### Step 3: Rename Type Definition

**Change** (line ~12):
```sql
-- OLD
CREATE TYPE mutation_result_v2 AS (

-- NEW
CREATE TYPE mutation_response AS (
```

### Step 4: Update All Helper Function Return Types

**Functions to update** (find with `grep "RETURNS mutation_result_v2"`):

1. `mutation_success()` - line ~34
2. `mutation_created()` - line ~61
3. `mutation_updated()` - line ~87
4. `mutation_deleted()` - line ~114
5. `mutation_noop()` - line ~138
6. `mutation_validation_error()` - line ~162
7. `mutation_not_found()` - line ~197
8. `mutation_conflict()` - line ~234
9. `mutation_error()` - line ~261

**Change for ALL**:
```sql
-- OLD
RETURNS mutation_result_v2 AS $$

-- NEW
RETURNS mutation_response AS $$
```

### Step 5: Update All Type Casts

**Find all casts** (search for `)::mutation_result_v2`):

**Change ALL occurrences**:
```sql
-- OLD
)::mutation_result_v2;

-- NEW
)::mutation_response;
```

**Expected locations**: ~20 occurrences in helper functions and examples

### Step 6: Update Utility Functions

**Functions** (lines 281-319):
- `mutation_is_success(result mutation_result_v2)`
- `mutation_is_error(result mutation_result_v2)`
- `mutation_is_noop(result mutation_result_v2)`
- `mutation_error_type(result mutation_result_v2)`
- `mutation_noop_reason(result mutation_result_v2)`

**Change parameter type for ALL**:
```sql
-- OLD
CREATE OR REPLACE FUNCTION mutation_is_success(result mutation_result_v2) RETURNS boolean

-- NEW
CREATE OR REPLACE FUNCTION mutation_is_success(result mutation_response) RETURNS boolean
```

### Step 7: Update Example Comments

**In the commented example section** (lines 536-707):

**Find and replace**:
```sql
-- OLD
RETURNS mutation_result_v2 AS $$

-- NEW
RETURNS mutation_response AS $$
```

**Verification**:
```bash
# No v2 references should remain
! grep -i "mutation_result_v2" migrations/trinity/005_add_mutation_response.sql

# Should find many mutation_response references
grep -c "mutation_response" migrations/trinity/005_add_mutation_response.sql
# Expected: 30+
```

---

## Task 1.2: Update v2_init.sql Example File

**File**: `examples/mutations_demo/v2_init.sql`

### Step 1: Update File Header
```sql
-- OLD (line ~1)
-- Updated init.sql using mutation_result_v2 format

-- NEW
-- Updated init.sql using mutation_response format
```

### Step 2: Global Replace

**Find ALL occurrences**:
```bash
grep -n "mutation_result_v2" examples/mutations_demo/v2_init.sql
```

**Replace ALL with**:
- `mutation_result_v2` → `mutation_response`

**Expected changes**:
- Type definition: `CREATE TYPE mutation_response`
- Function return types: `RETURNS mutation_response`
- Type casts: `)::mutation_response`

**Verification**:
```bash
! grep -i "mutation_result_v2" examples/mutations_demo/v2_init.sql
grep -c "mutation_response" examples/mutations_demo/v2_init.sql
# Expected: 10+
```

---

## Task 1.3: Update v2_mutation_functions.sql Example File

**File**: `examples/mutations_demo/v2_mutation_functions.sql`

### Step 1: Update Comments

**Find all comments mentioning the type**:
```bash
grep -n "mutation_result_v2" examples/mutations_demo/v2_mutation_functions.sql
```

### Step 2: Global Replace

**Replace ALL**:
- `mutation_result_v2` → `mutation_response`

**Expected changes**:
- Function return types
- Type casts
- Comments

**Verification**:
```bash
! grep -i "mutation_result_v2" examples/mutations_demo/v2_mutation_functions.sql
grep -c "mutation_response" examples/mutations_demo/v2_mutation_functions.sql
# Expected: 5+
```

---

## Phase 1 Verification

### Check All SQL Files

```bash
cd /home/lionel/code/fraiseql

# 1. No v2 references in any SQL files
! grep -r "mutation_result_v2" migrations/
! grep -r "mutation_result_v2" examples/mutations_demo/

# 2. Confirm mutation_response exists
grep -r "mutation_response" migrations/ | wc -l
# Expected: 30+

grep -r "mutation_response" examples/mutations_demo/ | wc -l
# Expected: 15+
```

### Test SQL Syntax

```bash
# If you have PostgreSQL installed locally, test the migration:
psql -d test_db -f migrations/trinity/005_add_mutation_response.sql
# Should execute without syntax errors
```

---

## Acceptance Criteria

- [ ] Migration file renamed to `005_add_mutation_response.sql`
- [ ] Old file `005_add_mutation_result_v2.sql` deleted/moved
- [ ] Type definition uses `mutation_response`
- [ ] All 9 helper functions return `mutation_response`
- [ ] All 5 utility functions accept `mutation_response` parameter
- [ ] All type casts use `::mutation_response`
- [ ] Example files updated (`v2_init.sql`, `v2_mutation_functions.sql`)
- [ ] Zero `mutation_result_v2` references in SQL files
- [ ] SQL syntax is valid (no typos)

---

## Git Commit

After verification passes:

```bash
git add migrations/trinity/
git add examples/mutations_demo/
git commit -m "refactor(db): rename mutation_result_v2 to mutation_response in PostgreSQL

- Rename migration file to 005_add_mutation_response.sql
- Update all helper functions return types
- Update example SQL files
- Update all type casts and comments

This is a pre-release rename with no external impact."
```

---

## Rollback

If issues are found:

```bash
# Option 1: Revert commit
git revert HEAD

# Option 2: Reset to before this phase
git reset --hard HEAD~1
```

---

## Next Steps

✅ **Proceed to Phase 2: Rust Layer Updates**

Location: `.phases/mutation_response_rename/phase2_rust.md`

---

**Phase Status**: ⏸️ Ready to Start
**Estimated Time**: 2 hours
**Dependencies**: Phase 0 complete
