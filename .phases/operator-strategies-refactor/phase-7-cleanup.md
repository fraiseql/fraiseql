# Phase 7: Legacy Cleanup - COMPLETE IMPLEMENTATION PLAN

**Phase:** CLEANUP (Remove Deprecated Code)
**Duration:** 2-3 hours
**Risk:** Low (but requires careful execution)
**Status:** Ready for Execution

---

## Objective

**TDD Phase CLEANUP:** Remove old monolithic file, update all imports, finalize API, and ensure clean codebase.

This phase completes the migration by:
- Deleting `/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py` (2,149 lines)
- Updating all imports across the codebase (tests, integration code, examples)
- Removing deprecation warnings and compatibility shims
- Verifying no references to old module remain
- Final smoke testing to ensure everything works

**Success:** Clean codebase with zero references to old `operator_strategies.py`, all tests passing.

---

## Context

**Current State:**
- ✅ Phase 1-4: All operator strategies migrated to modular structure
- ✅ Phase 5: Refactored and optimized
- ✅ Phase 6: QA validated (all 4,943+ tests passing)
- ⏳ Old file still exists: `/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py` (86,860 bytes)

**Files importing old module (found 19+ files):**
```
tests/regression/where_clause/test_industrial_where_clause_generation.py
tests/regression/where_clause/test_sql_structure_validation.py
tests/regression/where_clause/test_precise_sql_validation.py
tests/regression/where_clause/test_complete_sql_validation.py
tests/regression/where_clause/test_numeric_consistency_validation.py
tests/regression/v0_5_7/test_network_filtering_regression.py
tests/integration/repository/test_repository_find_where_processing.py
tests/integration/database/sql/test_jsonb_network_filtering_bug.py
tests/integration/database/sql/test_coordinate_filter_operations.py
tests/integration/database/sql/test_ltree_filter_operations.py
tests/integration/database/sql/test_network_operator_consistency_bug.py
tests/integration/database/sql/test_network_filtering_fix.py
tests/integration/database/sql/test_production_cqrs_ip_filtering_bug.py
tests/integration/database/sql/test_network_address_filtering.py
tests/integration/database/sql/test_mac_address_filter_operations.py
tests/integration/database/sql/test_daterange_filter_operations.py
tests/integration/database/sql/test_issue_resolution_demonstration.py
tests/unit/graphql/test_field_type_extraction.py
tests/unit/sql/test_network_operator_strategy_fix.py
tests/unit/sql/where/test_ltree_path_manipulation_operators.py
```

**Strategy:**
1. Find all references to old module
2. Update imports systematically (test files, then source files)
3. Backup old file, then delete it
4. Verify no references remain
5. Run full test suite
6. Commit cleanup

---

## Implementation Steps

### Step 1: Find All References (15 min)

**Goal:** Get complete inventory of all files referencing `operator_strategies`.

#### 1.1 Find Import Statements

```bash
# Find all Python files importing from operator_strategies
grep -r "from.*operator_strategies import\|import.*operator_strategies" \
    /home/lionel/code/fraiseql/src/ \
    /home/lionel/code/fraiseql/tests/ \
    --include="*.py" \
    > /tmp/phase-7-imports.txt

# Display results
cat /tmp/phase-7-imports.txt

# Count total files
echo "Total files to update: $(cat /tmp/phase-7-imports.txt | cut -d: -f1 | sort -u | wc -l)"
```

**Expected:** 19-25 files with imports to update

#### 1.2 Find String References

```bash
# Find string references to "operator_strategies" (in error messages, docs, comments)
grep -r "operator_strategies" \
    /home/lionel/code/fraiseql/src/ \
    /home/lionel/code/fraiseql/tests/ \
    /home/lionel/code/fraiseql/docs/ \
    --include="*.py" --include="*.md" --include="*.rst" \
    > /tmp/phase-7-string-refs.txt

# Display results
cat /tmp/phase-7-string-refs.txt

# Count references
echo "Total string references: $(wc -l < /tmp/phase-7-string-refs.txt)"
```

**Expected:** 25-40 references (imports + docstrings + error messages + comments)

#### 1.3 Create Update Plan

```bash
# Organize files by category
cat > /tmp/phase-7-update-plan.txt << 'EOF'
# Phase 7 Cleanup - File Update Plan

## Category 1: Test Files (19 files)
- Update imports: operator_strategies → operators
- Update strategy class references
- Verify tests still pass after each file

## Category 2: Source Files (estimated 2-5 files)
- Check src/fraiseql/sql/ for imports
- Check src/fraiseql/ root for imports
- Update any internal usage

## Category 3: Documentation Files (estimated 0-3 files)
- Update any references in docs/
- Update examples/
- Update README.md if needed

## Category 4: Error Messages (estimated 5-10 occurrences)
- Update error messages mentioning operator_strategies
- Update deprecation warnings
- Update module path references

EOF

cat /tmp/phase-7-update-plan.txt
```

**Acceptance:**
- [ ] All import statements found and documented
- [ ] All string references found and documented
- [ ] Update plan created with categories

---

### Step 2: Update Test Files (45 min)

**Goal:** Update all test file imports to use new `operators` module.

#### 2.1 Update Pattern Reference

**Common import patterns to update:**

```python
# PATTERN 1: Direct imports
# OLD:
from fraiseql.sql.operator_strategies import (
    NetworkOperatorStrategy,
    StringOperatorStrategy,
    BaseOperatorStrategy,
)

# NEW:
from fraiseql.sql.operators import (
    NetworkOperatorStrategy,
    StringOperatorStrategy,
    BaseOperatorStrategy,
)

# PATTERN 2: Module import
# OLD:
from fraiseql.sql import operator_strategies

# NEW:
from fraiseql.sql import operators

# PATTERN 3: Strategy instantiation
# OLD:
strategy = operator_strategies.NetworkOperatorStrategy()

# NEW:
from fraiseql.sql.operators import NetworkOperatorStrategy
strategy = NetworkOperatorStrategy()

# PATTERN 4: Registry usage
# OLD:
# (may not exist - old code didn't have registry pattern)

# NEW:
from fraiseql.sql.operators import get_default_registry
registry = get_default_registry()
```

#### 2.2 Update Regression Tests (20 min)

```bash
# Update WHERE clause regression tests
cd /home/lionel/code/fraiseql

# File 1: test_industrial_where_clause_generation.py
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    tests/regression/where_clause/test_industrial_where_clause_generation.py

# File 2: test_sql_structure_validation.py
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    tests/regression/where_clause/test_sql_structure_validation.py

# File 3: test_precise_sql_validation.py
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    tests/regression/where_clause/test_precise_sql_validation.py

# File 4: test_complete_sql_validation.py
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    tests/regression/where_clause/test_complete_sql_validation.py

# File 5: test_numeric_consistency_validation.py
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    tests/regression/where_clause/test_numeric_consistency_validation.py

# File 6: test_network_filtering_regression.py
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    tests/regression/v0_5_7/test_network_filtering_regression.py

# Verify changes
grep -n "from fraiseql.sql.operators import" tests/regression/where_clause/*.py tests/regression/v0_5_7/*.py

# Run regression tests to verify
uv run pytest tests/regression/ -v
```

**Expected:** All regression tests pass with updated imports

**Acceptance:**
- [ ] 6 regression test files updated
- [ ] All regression tests passing

#### 2.3 Update Integration Tests (20 min)

```bash
cd /home/lionel/code/fraiseql

# Update all integration test files
for file in \
    tests/integration/repository/test_repository_find_where_processing.py \
    tests/integration/database/sql/test_jsonb_network_filtering_bug.py \
    tests/integration/database/sql/test_coordinate_filter_operations.py \
    tests/integration/database/sql/test_ltree_filter_operations.py \
    tests/integration/database/sql/test_network_operator_consistency_bug.py \
    tests/integration/database/sql/test_network_filtering_fix.py \
    tests/integration/database/sql/test_production_cqrs_ip_filtering_bug.py \
    tests/integration/database/sql/test_network_address_filtering.py \
    tests/integration/database/sql/test_mac_address_filter_operations.py \
    tests/integration/database/sql/test_daterange_filter_operations.py \
    tests/integration/database/sql/test_issue_resolution_demonstration.py
do
    echo "Updating: $file"
    sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' "$file"
    sed -i 's/import fraiseql\.sql\.operator_strategies/import fraiseql.sql.operators/g' "$file"
done

# Verify changes
grep -n "from fraiseql.sql.operators import" tests/integration/database/sql/*.py | head -20

# Run integration tests to verify
uv run pytest tests/integration/database/sql/ -v -k "network or ltree or daterange or mac or coordinate or jsonb"
```

**Expected:** All updated integration tests pass

**Acceptance:**
- [ ] 11 integration test files updated
- [ ] Integration tests passing with new imports

#### 2.4 Update Unit Tests (5 min)

```bash
cd /home/lionel/code/fraiseql

# Update unit test files
for file in \
    tests/unit/graphql/test_field_type_extraction.py \
    tests/unit/sql/test_network_operator_strategy_fix.py \
    tests/unit/sql/where/test_ltree_path_manipulation_operators.py
do
    echo "Updating: $file"
    sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' "$file"
done

# Verify changes
grep -n "from fraiseql.sql.operators import" tests/unit/graphql/*.py tests/unit/sql/*.py tests/unit/sql/where/*.py

# Run unit tests to verify
uv run pytest tests/unit/ -v -k "network or ltree or field_type"
```

**Expected:** All updated unit tests pass

**Acceptance:**
- [ ] 3 unit test files updated
- [ ] Unit tests passing with new imports

---

### Step 3: Update Source Files (30 min)

**Goal:** Update any source code files that import from `operator_strategies`.

#### 3.1 Check Source File Imports

```bash
cd /home/lionel/code/fraiseql

# Find any source files importing operator_strategies
grep -r "from.*operator_strategies import\|import.*operator_strategies" \
    src/ \
    --include="*.py" \
    --exclude-dir=operators

# Expected: Should find zero or very few (possibly old WHERE generator)
```

**Possible files to update:**
- `src/fraiseql/sql/graphql_where_generator.py` - GraphQL filter to WHERE clause
- `src/fraiseql/sql/where_generator.py` - WHERE clause builder (if exists)
- `src/fraiseql/where_clause.py` - WHERE clause objects (if exists)
- `src/fraiseql/db.py` - Repository database methods

#### 3.2 Update WHERE Generator (if needed)

```bash
# Check if graphql_where_generator imports operator_strategies
grep -n "operator_strategies" src/fraiseql/sql/graphql_where_generator.py

# If it does, update it:
sed -i 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' \
    src/fraiseql/sql/graphql_where_generator.py

# Verify
grep -n "from fraiseql.sql.operators" src/fraiseql/sql/graphql_where_generator.py
```

**Expected:** WHERE generator may already be using new module (check first)

#### 3.3 Update SQL Module __init__.py (if needed)

```bash
# Check sql module exports
cat src/fraiseql/sql/__init__.py

# If it exports operator_strategies, update:
# BEFORE:
#   from .operator_strategies import OperatorStrategy
# AFTER:
#   from .operators import get_default_registry, BaseOperatorStrategy

# Update if needed
nano src/fraiseql/sql/__init__.py
# Or use sed if pattern is clear
```

**Expected:** May already be updated in Phase 2/3

#### 3.4 Verify Source Changes

```bash
# Run quick import test
python3 -c "
from fraiseql.sql.operators import get_default_registry, BaseOperatorStrategy
from fraiseql.sql import operators
print('Import test: OK')
print('Registry:', get_default_registry())
print('Base class:', BaseOperatorStrategy)
"

# Expected output:
# Import test: OK
# Registry: <fraiseql.sql.operators.strategy_registry.OperatorRegistry object at 0x...>
# Base class: <class 'fraiseql.sql.operators.base.BaseOperatorStrategy'>
```

**Acceptance:**
- [ ] All source file imports updated
- [ ] Import test passes
- [ ] No import errors

---

### Step 4: Update Error Messages & Docstrings (20 min)

**Goal:** Update string references to old module in error messages, docstrings, comments.

#### 4.1 Find String References

```bash
cd /home/lionel/code/fraiseql

# Find all string references (excluding the old file itself)
grep -r "operator_strategies" \
    src/ tests/ \
    --include="*.py" \
    --exclude="operator_strategies.py" \
    > /tmp/phase-7-string-refs-detailed.txt

cat /tmp/phase-7-string-refs-detailed.txt
```

**Common patterns:**

```python
# PATTERN 1: Error messages
# OLD:
raise ValueError("Invalid operator. See fraiseql.sql.operator_strategies for supported operators.")

# NEW:
raise ValueError("Invalid operator. See fraiseql.sql.operators documentation for supported operators.")

# PATTERN 2: Docstring references
# OLD:
"""
Uses operator_strategies to build SQL.
See: fraiseql.sql.operator_strategies
"""

# NEW:
"""
Uses operator strategies to build SQL.
See: fraiseql.sql.operators
"""

# PATTERN 3: Comment references
# OLD:
# Import from operator_strategies

# NEW:
# Import from operators module
```

#### 4.2 Update Error Messages

```bash
cd /home/lionel/code/fraiseql

# Find files with error messages mentioning operator_strategies
grep -r "operator_strategies" src/ tests/ --include="*.py" | grep -E "raise|ValueError|TypeError|Error"

# Update each file manually or with sed (if pattern is consistent)
# Example:
find src/ tests/ -name "*.py" -type f -exec sed -i \
    's/fraiseql\.sql\.operator_strategies/fraiseql.sql.operators/g' {} +
```

#### 4.3 Update Docstrings & Comments

```bash
# Update any remaining references in docstrings/comments
find src/ tests/ -name "*.py" -type f -exec sed -i \
    's/operator_strategies module/operators module/g' {} +

find src/ tests/ -name "*.py" -type f -exec sed -i \
    's/from operator_strategies/from operators module/g' {} +
```

**Acceptance:**
- [ ] All error messages updated
- [ ] All docstring references updated
- [ ] All comment references updated

---

### Step 5: Update Documentation Files (15 min)

**Goal:** Update any documentation referencing old module.

#### 5.1 Check Documentation

```bash
cd /home/lionel/code/fraiseql

# Find documentation files mentioning operator_strategies
find docs/ -name "*.md" -o -name "*.rst" 2>/dev/null | xargs grep -l "operator_strategies" 2>/dev/null

# Check README
grep -n "operator_strategies" README.md 2>/dev/null || echo "README.md: No references found"

# Check examples
find examples/ -name "*.py" 2>/dev/null | xargs grep -l "operator_strategies" 2>/dev/null || echo "examples/: No references found"
```

**Expected:** May find 0-3 files to update

#### 5.2 Update Documentation (if found)

```bash
# Update any documentation files
# Manual editing recommended for documentation to ensure quality

# If automated update needed:
find docs/ -name "*.md" -type f -exec sed -i \
    's/fraiseql\.sql\.operator_strategies/fraiseql.sql.operators/g' {} + 2>/dev/null

# Update README if needed
sed -i 's/fraiseql\.sql\.operator_strategies/fraiseql.sql.operators/g' README.md 2>/dev/null
```

**Acceptance:**
- [ ] Documentation files updated (if any found)
- [ ] README updated (if needed)
- [ ] Examples updated (if any found)

---

### Step 6: Backup and Delete Old File (10 min)

**Goal:** Remove the old monolithic `operator_strategies.py` file.

#### 6.1 Create Backup

```bash
cd /home/lionel/code/fraiseql

# Create backup with timestamp
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
cp src/fraiseql/sql/operator_strategies.py \
   /tmp/operator_strategies_backup_${TIMESTAMP}.py

# Verify backup exists
ls -lh /tmp/operator_strategies_backup_${TIMESTAMP}.py

# Expected: ~86 KB file
```

**Acceptance:**
- [ ] Backup created successfully
- [ ] Backup file size ~86 KB

#### 6.2 Delete Old File

```bash
cd /home/lionel/code/fraiseql

# Remove old file from git
git rm src/fraiseql/sql/operator_strategies.py

# Verify removal
ls src/fraiseql/sql/operator_strategies.py 2>&1
# Expected: "No such file or directory"

# Check git status
git status | grep operator_strategies
# Expected: Shows file as deleted
```

**Acceptance:**
- [ ] Old file removed from filesystem
- [ ] Git shows file as deleted
- [ ] Backup exists in /tmp

---

### Step 7: Verify No References Remain (15 min)

**Goal:** Ensure zero references to old module exist.

#### 7.1 Search for Remaining References

```bash
cd /home/lionel/code/fraiseql

# Search entire codebase for "operator_strategies"
grep -r "operator_strategies" src/ tests/ docs/ --include="*.py" --include="*.md" --include="*.rst" 2>/dev/null

# Expected: Zero results (or only in backup/archive files)
```

**If results found:**
- Review each result
- Update if it's a real reference
- Ignore if it's in comments/history/archive

#### 7.2 Verify Import Fails

```bash
# Try to import old module (should fail)
python3 -c "from fraiseql.sql.operator_strategies import BaseOperatorStrategy" 2>&1

# Expected output:
# ModuleNotFoundError: No module named 'fraiseql.sql.operator_strategies'

# Verify correct import works
python3 -c "from fraiseql.sql.operators import BaseOperatorStrategy; print('OK')"

# Expected output:
# OK
```

**Acceptance:**
- [ ] Zero references to `operator_strategies` found
- [ ] Old import fails with ModuleNotFoundError
- [ ] New import succeeds

---

### Step 8: Run Full Test Suite (20 min)

**Goal:** Verify all tests pass with updated imports and deleted file.

#### 8.1 Quick Unit Tests

```bash
cd /home/lionel/code/fraiseql

# Run unit tests
uv run pytest tests/unit/ -v --tb=short

# Expected: All unit tests passing
```

#### 8.2 Integration Tests

```bash
# Run integration tests
uv run pytest tests/integration/ -v --tb=short

# Expected: All integration tests passing
```

#### 8.3 Regression Tests

```bash
# Run regression tests
uv run pytest tests/regression/ -v --tb=short

# Expected: All regression tests passing
```

#### 8.4 Full Test Suite

```bash
# Run FULL test suite
uv run pytest tests/ -v --tb=short

# Count results
uv run pytest tests/ -v --tb=short | grep -E "passed|failed|error" | tail -3

# Expected: 4,943+ passed, 0 failed, 0 errors
```

**If tests fail:**
1. Check error message for import errors
2. Find the file with the issue
3. Update the import
4. Re-run tests
5. Repeat until all pass

**Acceptance:**
- [ ] All unit tests passing
- [ ] All integration tests passing
- [ ] All regression tests passing
- [ ] Full test suite passing (4,943+ tests)
- [ ] Zero import errors
- [ ] Zero ModuleNotFoundError exceptions

---

### Step 9: Final Smoke Tests (10 min)

**Goal:** Quick manual verification of key functionality.

#### 9.1 Import Test

```bash
python3 << 'EOF'
# Test all key imports work
from fraiseql.sql.operators import (
    get_default_registry,
    BaseOperatorStrategy,
    StringOperatorStrategy,
    NumericOperatorStrategy,
    BooleanOperatorStrategy,
    NetworkOperatorStrategy,
    LTreeOperatorStrategy,
    DateRangeOperatorStrategy,
    MacAddressOperatorStrategy,
)

print("✅ All imports successful")

# Test registry
registry = get_default_registry()
print(f"✅ Registry: {registry}")
print(f"✅ Registry has {len(registry._strategies)} strategies")
EOF
```

**Expected output:**
```
✅ All imports successful
✅ Registry: <fraiseql.sql.operators.strategy_registry.OperatorRegistry object at 0x...>
✅ Registry has 7 strategies
```

#### 9.2 Operator Test

```bash
python3 << 'EOF'
from psycopg.sql import Identifier
from fraiseql.sql.operators import get_default_registry

registry = get_default_registry()

# Test string operator
result = registry.build_sql("contains", "test", Identifier("name"), field_type=str)
print(f"✅ String operator: {result.as_string(None)}")

# Test numeric operator
result = registry.build_sql("gt", 42, Identifier("age"), field_type=int)
print(f"✅ Numeric operator: {result.as_string(None)}")

# Test network operator
from ipaddress import IPv4Address
result = registry.build_sql("isprivate", None, Identifier("ip"), field_type=IPv4Address)
print(f"✅ Network operator: {result.as_string(None)}")

print("\n✅ All smoke tests passed!")
EOF
```

**Expected output:**
```
✅ String operator: CAST("name" AS TEXT) LIKE '%test%'
✅ Numeric operator: "age" > 42
✅ Network operator: NOT inet_public(CAST("ip" AS inet))

✅ All smoke tests passed!
```

#### 9.3 Integration Smoke Test

```bash
# Run a quick end-to-end integration test
uv run pytest tests/integration/database/sql/test_graphql_where_generator.py::test_simple_string_filter -v 2>/dev/null \
    || uv run pytest tests/integration/database/sql/ -k "simple" -v --maxfail=1

# Expected: Test passes
```

**Acceptance:**
- [ ] Import test passes
- [ ] Operator test passes
- [ ] Integration smoke test passes
- [ ] No errors or exceptions

---

## Acceptance Criteria Summary

### File Updates
- [ ] All 19+ test files updated with new imports
- [ ] All source files updated (if any found)
- [ ] All error messages updated
- [ ] All documentation files updated (if any found)

### Old File Removal
- [ ] Old `operator_strategies.py` backed up to /tmp
- [ ] Old `operator_strategies.py` deleted via `git rm`
- [ ] Git shows file as deleted

### Verification
- [ ] Zero references to `operator_strategies` in codebase
- [ ] Old import fails with ModuleNotFoundError
- [ ] New imports work correctly
- [ ] Full test suite passing (4,943+ tests)
- [ ] All regression tests passing
- [ ] Smoke tests passing

### Code Quality
- [ ] No import errors
- [ ] No circular imports
- [ ] Clean git status (ready to commit)

---

## Rollback Plan

**If issues found after deletion:**

### Option 1: Restore from backup

```bash
# Restore old file
cp /tmp/operator_strategies_backup_*.py src/fraiseql/sql/operator_strategies.py

# Add back to git
git add src/fraiseql/sql/operator_strategies.py

# Revert all import changes
git checkout HEAD -- tests/ src/

# Run tests to verify
uv run pytest tests/ -v
```

### Option 2: Restore from git history

```bash
# Restore from previous commit
git checkout HEAD~1 -- src/fraiseql/sql/operator_strategies.py

# Run tests
uv run pytest tests/ -v
```

### Option 3: Revert entire cleanup commit

```bash
# If already committed, revert the commit
git revert HEAD

# Or reset to before cleanup
git reset --hard HEAD~1
```

**When to rollback:**
- If > 10 test failures after cleanup
- If critical production code breaks
- If import errors cannot be resolved quickly
- If issues found are blocking other work

**Prevention:**
- Commit each step (Step 2 → commit, Step 3 → commit, etc.)
- Run tests after each major change
- Keep backup until Phase 8 complete
- Don't push to production until full QA (Phase 6) passes

---

## Commit Strategy

**Commit after each major step:**

```bash
# After Step 2 (test files updated)
git add tests/
git commit -m "refactor(tests): update operator_strategies imports to operators [CLEANUP]

Update all test imports from operator_strategies to operators:
- Updated 6 regression test files
- Updated 11 integration test files
- Updated 3 unit test files

All tests passing with new imports."

# After Step 3 (source files updated)
git add src/
git commit -m "refactor(sql): update operator_strategies imports in source code [CLEANUP]

Update source file imports:
- Update WHERE generator imports (if applicable)
- Update SQL module exports (if applicable)
- Update repository code (if applicable)

All imports now use fraiseql.sql.operators module."

# After Step 4-5 (error messages & docs updated)
git add src/ tests/ docs/ README.md
git commit -m "docs: update operator_strategies references in messages and docs [CLEANUP]

Update all string references:
- Error messages reference fraiseql.sql.operators
- Docstrings updated
- Comments updated
- Documentation files updated (if any)

Zero references to old module path remain."

# After Step 6 (old file deleted)
git add src/fraiseql/sql/operator_strategies.py
git commit -m "refactor(sql): remove legacy operator_strategies.py [CLEANUP]

BREAKING CHANGE: Monolithic operator_strategies.py removed.
All operator strategies now in fraiseql.sql.operators module.

Migration:
- OLD: from fraiseql.sql.operator_strategies import X
- NEW: from fraiseql.sql.operators import X

Changes:
- Deleted: src/fraiseql/sql/operator_strategies.py (2,149 lines, 86KB)
- Backup: /tmp/operator_strategies_backup_*.py
- Updated: 19+ test files, source files, documentation

All 4,943+ tests passing. Migration complete.

Phase 7 (CLEANUP) complete.
Next: Phase 8 (Documentation)"
```

---

## Troubleshooting Guide

### Issue 1: Import Errors After Deletion

**Symptom:**
```
ModuleNotFoundError: No module named 'fraiseql.sql.operator_strategies'
```

**Solution:**
```bash
# Find the file with the issue
grep -r "operator_strategies" tests/ src/ --include="*.py"

# Update the import in that file
sed -i 's/from fraiseql\.sql\.operator_strategies/from fraiseql.sql.operators/g' <file>

# Re-run tests
uv run pytest <file> -v
```

### Issue 2: Test Failures After Update

**Symptom:**
```
AttributeError: module 'fraiseql.sql.operators' has no attribute 'build_operator_sql'
```

**Cause:** Old code using old API that doesn't exist in new module

**Solution:**
```python
# OLD API (doesn't exist):
from fraiseql.sql.operators import build_operator_sql
result = build_operator_sql(op, value, path)

# NEW API:
from fraiseql.sql.operators import get_default_registry
registry = get_default_registry()
result = registry.build_sql(op, value, path, field_type=field_type)
```

### Issue 3: Circular Import Errors

**Symptom:**
```
ImportError: cannot import name 'X' from partially initialized module 'fraiseql.sql.operators'
```

**Cause:** Circular dependency between modules

**Solution:**
```bash
# Check import chain
python3 -c "import fraiseql.sql.operators" 2>&1 | grep "ImportError"

# Fix by moving import to function scope or using TYPE_CHECKING
# See fraiseql/sql/operators/__init__.py
```

### Issue 4: Strategy Not Found

**Symptom:**
```python
registry.build_sql("isprivate", ...) returns None
```

**Cause:** Strategy not auto-registered

**Solution:**
```bash
# Check operators/__init__.py has register calls:
grep "register_operator" src/fraiseql/sql/operators/__init__.py

# Should see:
# register_operator(NetworkOperatorStrategy())
# register_operator(StringOperatorStrategy())
# etc.
```

---

## Next Phase

Once cleanup is complete and all acceptance criteria are met:

→ **Phase 8:** Documentation (`/tmp/phase-8-documentation-COMPLETE.md`)

**Prerequisites for Phase 8:**
- All test files updated ✅
- All source files updated ✅
- Old file deleted ✅
- Zero references to old module ✅
- All 4,943+ tests passing ✅
- Changes committed ✅

**Phase 8 will:**
- Write comprehensive operator architecture documentation
- Create migration guide for users
- Update API reference documentation
- Add examples for each operator family
- Update CHANGELOG with breaking changes
- Update README and other top-level docs

---

## Files Modified Summary

**Estimated files modified in this phase:**

| Category | Files | Lines Changed |
|----------|-------|---------------|
| Test files | 19+ | ~50 lines (import updates) |
| Source files | 2-5 | ~20 lines (import updates) |
| Documentation | 0-3 | ~30 lines (path updates) |
| Error messages | 5-10 occurrences | ~15 lines (string updates) |
| **Old file deleted** | **1 file** | **-2,149 lines** |
| **Net change** | **~25 files** | **~-2,050 lines** |

**Result:**
- Cleaner codebase (removed 2,149 lines of duplicated code)
- All functionality preserved (zero behavior changes)
- All tests passing (zero regressions)
- Ready for documentation (Phase 8)

---

## Notes

**Why delete now?**
- All operator strategies migrated (Phases 1-4)
- All code refactored and optimized (Phase 5)
- All tests passing and QA complete (Phase 6)
- Safe to remove legacy code
- Prevents confusion about which module to use

**Why keep backup?**
- Safety net in case rollback needed
- Reference for any missed functionality
- Can be deleted after Phase 8 complete
- Useful for diffing if issues arise

**What if we find issues in Phase 8?**
- Phase 8 is documentation only (no code changes)
- If issues found, fix them before finalizing Phase 8
- Re-run Phase 7 verification after fixes
- Update Phase 8 docs to reflect any changes

**Migration impact:**
- Internal to FraiseQL (not public API)
- Test code only (not production code)
- If FraiseQL is used as library, users unaffected (public API unchanged)
- If contributors reference old module, migration guide needed (Phase 8)
