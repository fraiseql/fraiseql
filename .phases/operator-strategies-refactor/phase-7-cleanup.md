# Phase 7: Legacy Cleanup

**Phase:** CLEANUP (Remove Deprecated Code)
**Duration:** 2-3 hours
**Risk:** Low

---

## Objective

**TDD Phase CLEANUP:** Remove old code, finalize API, update all imports.

Remove:
- `src/fraiseql/sql/operator_strategies.py` (old monolithic file)
- Deprecation warnings
- Backward compatibility shims
- Dead code

Update:
- All imports across codebase
- Public API exports
- Error messages referencing old module

**Success:** Clean codebase with no references to old operator_strategies.py

---

## Implementation Steps

### Step 1: Find All References (30 min)

```bash
# Find all imports of old module
grep -r "from fraiseql.sql.operator_strategies import" src/ tests/
grep -r "import fraiseql.sql.operator_strategies" src/ tests/

# Find all references in documentation
grep -r "operator_strategies" docs/

# Find references in error messages
grep -r "operator_strategies" src/ --include="*.py"
```

### Step 2: Update Imports (1 hour)

For each file found:

**BEFORE:**
```python
from fraiseql.sql.operator_strategies import OperatorStrategy
```

**AFTER:**
```python
from fraiseql.sql.operators import get_default_registry
```

**Pattern replacements:**
```python
# OLD: Direct operator_strategies usage
from fraiseql.sql.operator_strategies import build_operator_sql
result = build_operator_sql(op, value, path)

# NEW: Registry-based usage
from fraiseql.sql.operators import get_default_registry
registry = get_default_registry()
result = registry.build_sql(op, value, path)
```

### Step 3: Delete Old File (15 min)

```bash
# Backup first (just in case)
cp src/fraiseql/sql/operator_strategies.py /tmp/operator_strategies_backup.py

# Remove old file
git rm src/fraiseql/sql/operator_strategies.py
```

### Step 4: Update Error Messages (30 min)

Find and update all error messages that reference the old module:

```python
# BEFORE
raise ValueError("Invalid operator. See fraiseql.sql.operator_strategies for supported operators.")

# AFTER
raise ValueError("Invalid operator. See fraiseql.sql.operators documentation for supported operators.")
```

### Step 5: Remove Deprecation Warnings (15 min)

Remove any deprecation warnings added during migration:

```python
# BEFORE (in __init__.py or other files)
warnings.warn(
    "operator_strategies is deprecated, use fraiseql.sql.operators",
    DeprecationWarning
)

# AFTER - DELETE these warnings
```

---

## Files to Modify

### 1. Remove `src/fraiseql/sql/operator_strategies.py`

```bash
git rm src/fraiseql/sql/operator_strategies.py
```

### 2. Update imports in key files:

- `src/fraiseql/sql/graphql_where_generator.py`
- `src/fraiseql/sql/where_generator.py`
- `src/fraiseql/where_clause.py`
- `src/fraiseql/db.py`
- Any other files that import operator strategies

### 3. Update `src/fraiseql/sql/__init__.py`

Remove old exports:

```python
# BEFORE
from .operator_strategies import OperatorStrategy  # OLD

# AFTER
from .operators import get_default_registry, BaseOperatorStrategy
```

---

## Verification Commands

```bash
# Verify no references to old module exist
grep -r "operator_strategies" src/ tests/ docs/
# Should return zero matches

# Verify imports work
python -c "from fraiseql.sql.operators import get_default_registry; print('OK')"

# Run full test suite
uv run pytest --tb=short -v

# Check that old imports fail properly
python -c "from fraiseql.sql.operator_strategies import X" 2>&1 | grep "ModuleNotFoundError"
# Should show ModuleNotFoundError
```

---

## Rollback Plan

If issues found after deletion:

```bash
# Restore from backup
cp /tmp/operator_strategies_backup.py src/fraiseql/sql/operator_strategies.py

# Restore from git
git checkout HEAD -- src/fraiseql/sql/operator_strategies.py
```

---

## Acceptance Criteria

- [ ] `operator_strategies.py` deleted
- [ ] All imports updated to new module
- [ ] All tests passing with new imports
- [ ] No grep matches for "operator_strategies" in src/
- [ ] No deprecation warnings
- [ ] Error messages updated
- [ ] Documentation updated
- [ ] Clean git diff

---

## Commit Message Template

```
refactor(sql): remove legacy operator_strategies.py [CLEANUP]

Complete migration to modular operator strategy architecture.

BREAKING CHANGE: `fraiseql.sql.operator_strategies` module removed.
Use `fraiseql.sql.operators` instead.

Migration:
- OLD: from fraiseql.sql.operator_strategies import X
- NEW: from fraiseql.sql.operators import X

- Deleted: operator_strategies.py (2,149 lines)
- Added: operators/ directory with 12 focused modules
- Updated: all imports across codebase
- Tests: all 4,943 tests passing

Related commits:
- Phase 1 (RED): Foundation
- Phase 2 (GREEN): Core operators
- Phase 3 (GREEN): PostgreSQL types
- Phase 4 (GREEN): Advanced operators
- Phase 5 (REFACTOR): Optimization
- Phase 6 (QA): Verification
```

---

## Next Phase

Once cleanup is complete:
→ **Phase 8:** Documentation (update guides, examples, migration notes)
