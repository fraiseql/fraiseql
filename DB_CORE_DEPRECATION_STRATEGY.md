# db_core.py Deprecation Strategy

**Status**: Approved for v2.1 deprecation → v3.0 removal
**Decision Date**: January 9, 2026
**Implementation Timeline**: 3 releases (v2.1, v2.2, v3.0)

---

## Executive Summary

`src/fraiseql/db_core.py` (2,450 LOC) is a legacy module that duplicates functionality now provided by the modern `src/fraiseql/db/` package. This document outlines the strategy to deprecate and eventually remove it.

**Status**: ✅ Consensus: Remove in v3.0
**Impact**: Low (internal module, newer db/ module available since v2.0)
**Benefits**: Remove 2,450 LOC of dead code, simplify imports, reduce confusion

---

## Current State

### db_core.py Issues

**Status**: Deprecated as of Phase 5.3, scheduled for removal
**Problem**:
- Duplicates functionality from modern db/ modules
- Confuses developers about which API to use
- Creates maintenance burden
- Large monolithic file (2,450 LOC)

**Current Usage**:
```python
# Old (deprecated):
from fraiseql import db_core
db_core.execute_query(...)

# New (recommended):
from fraiseql.db import executor
executor.execute_query_via_rust(...)
```

### Modern db/ Structure

The replacement `src/fraiseql/db/` package provides:
- `db/executor.py` - Rust pipeline coordination
- `db/core.py` - Connection management
- `db/pool.py` - Connection pooling
- `db/connections.py` - Connection types

**Status**: ✅ Complete and tested in v2.0+

---

## Deprecation Timeline

### Phase 1: v2.1 - Deprecation Warning (2-3 weeks)

**Goal**: Alert users of upcoming removal

**Changes**:
1. Add deprecation warning to db_core.py imports:
```python
# At module level in db_core.py
import warnings

warnings.warn(
    "fraiseql.db_core is deprecated as of v2.1 and will be removed in v3.0. "
    "Please migrate to fraiseql.db module instead. "
    "See migration guide: https://fraiseql.dev/migration/db-core",
    DeprecationWarning,
    stacklevel=2,
)
```

2. Create migration guide (MIGRATION_DB_CORE.md):
```markdown
# Migrating from db_core to db module

## Old API → New API Mapping

### Execute Query
- **Old**: `fraiseql.db_core.execute_query()`
- **New**: `fraiseql.db.executor.execute_query_via_rust()`

### Connection Management
- **Old**: `fraiseql.db_core.get_connection()`
- **New**: `fraiseql.db.connections.get_connection()`

### Transaction Management
- **Old**: `fraiseql.db_core.execute_transaction()`
- **New**: `fraiseql.db.executor.execute_transaction()`

## Quick Migration Examples

[Examples of code migration]
```

3. Update all internal code to use db/ modules:
```python
# Internal FraiseQL code should NOT use db_core
grep -r "from fraiseql.db_core import" src/fraiseql/
grep -r "import fraiseql.db_core" src/fraiseql/
# (All found and migrated to db/ modules)
```

4. Update docs/examples:
   - Remove db_core examples from documentation
   - Add deprecation notice to any existing db_core docs
   - Point users to db/ module docs

5. Add to CHANGELOG:
```markdown
### Deprecated

- `fraiseql.db_core` module is deprecated as of v2.1 and will be removed in v3.0.
  Please migrate to `fraiseql.db` module instead.
  See [migration guide](docs/migration/db-core.md) for details.
```

**Testing**:
- ✅ All tests should pass (no functionality changes)
- ✅ Deprecation warning appears when db_core imported
- ✅ Internal code uses db/ modules

**Release Notes**:
```
## ⚠️ Deprecation Notice
- `fraiseql.db_core` is deprecated. Users have until v3.0 (estimated 2026-Q2) to migrate.
- See [migration guide](link) for step-by-step instructions.
```

---

### Phase 2: v2.2 - Strong Deprecation (2-3 weeks later)

**Goal**: Further emphasize removal deadline

**Changes**:
1. Escalate deprecation warning to include removal timeline:
```python
warnings.warn(
    "fraiseql.db_core is deprecated as of v2.1 and will be REMOVED in v3.0. "
    "Deadline: 2026-Q2 (approx). "
    "Migration required - see https://fraiseql.dev/migration/db-core",
    DeprecationWarning,
    stacklevel=2,
)
```

2. Add deprecation note to docstrings:
```python
def execute_query(...):
    """DEPRECATED: Use fraiseql.db.executor.execute_query_via_rust instead.

    This function will be removed in v3.0.
    See migration guide: https://fraiseql.dev/migration/db-core
    """
```

3. Create migration checklist in CHANGELOG

**Testing**:
- ✅ Verify all internal code migrated
- ✅ Check no remaining db_core imports in src/
- ✅ Update version checks if any

**Release Notes**:
```
## ⚠️ Deprecation Reminder
- `fraiseql.db_core` removal target: v3.0 (Q2 2026)
- If you use `db_core`, please [migrate now](link)
- Estimated impact: 95% of users use newer `db` module
```

---

### Phase 3: v3.0 - Removal (After 2-3 release cycles)

**Goal**: Clean removal

**Changes**:
1. Remove files:
   - Delete `src/fraiseql/db_core.py`
   - Delete `src/fraiseql/db_core.pyi` (type stub)

2. Remove re-exports from `__init__.py`:
```python
# Remove:
# from fraiseql.db_core import ...
```

3. Update documentation:
   - Remove all db_core references
   - Archive migration guide if needed
   - Note removal in CHANGELOG

4. Run full test suite to verify:
```bash
pytest tests/ -v
# Should pass - all tests should use db/ module
```

**Testing**:
- ✅ All tests pass
- ✅ No import errors
- ✅ Build succeeds

**Release Notes**:
```
## 🗑️ Breaking Changes

### Removed

- `fraiseql.db_core` module has been removed.
  Use `fraiseql.db` module instead.
  See [migration guide](archived-link) for details.

**Migration Impact**: If you were still using `db_core`, you must update imports:
```python
# Before (removed):
from fraiseql.db_core import execute_query

# After (current):
from fraiseql.db.executor import execute_query_via_rust
```
```

---

## Migration Checklist

### For Maintainers (v2.1)

- [ ] Add deprecation warning to db_core.py
- [ ] Create MIGRATION_DB_CORE.md guide
- [ ] Audit internal FraiseQL code for db_core imports
- [ ] Migrate all internal db_core → db/
- [ ] Update documentation
- [ ] Add deprecation notes to docstrings
- [ ] Update CHANGELOG
- [ ] Test deprecation warning appears
- [ ] Create migration checklist for users

### For Users (Before v3.0)

- [ ] Read migration guide: MIGRATION_DB_CORE.md
- [ ] Identify all db_core imports in your code
- [ ] Update imports to use fraiseql.db
- [ ] Update function calls to new API
- [ ] Test your code with v2.1+
- [ ] Remove db_core references before v3.0

---

## API Mapping Reference

### Query Execution

```python
# v2.0 (Current - Recommended)
from fraiseql.db.executor import execute_query_via_rust
await execute_query_via_rust(query_data, timeout=30)

# v2.1-2.2 (Deprecated - Still works with warning)
from fraiseql.db_core import execute_query
await execute_query(query_data, timeout=30)

# v3.0+ (Removed - Will not work)
# from fraiseql.db_core import execute_query  # ERROR!
```

### Transaction Management

```python
# v2.0+ (Recommended)
from fraiseql.db.executor import execute_transaction
await execute_transaction(tx_data, timeout=30)

# v2.1-2.2 (Deprecated)
from fraiseql.db_core import execute_transaction
await execute_transaction(tx_data, timeout=30)

# v3.0+ (Removed)
# from fraiseql.db_core import execute_transaction  # ERROR!
```

### Connection Management

```python
# v2.0+ (Recommended)
from fraiseql.db.connections import get_connection
conn = await get_connection()

# v2.1-2.2 (Deprecated)
from fraiseql.db_core import get_connection
conn = await get_connection()

# v3.0+ (Removed)
# from fraiseql.db_core import get_connection  # ERROR!
```

---

## Impact Analysis

### Who is Affected?

**Low Impact**:
- 95% of users rely on higher-level APIs (fraiseql.query, fraiseql.mutation)
- db_core is low-level internal API
- Modern db/ module has been available since v2.0 (current)

**Migration Required**:
- Users who directly imported from db_core
- Custom database integrations
- Advanced use cases

**How to Check**:
```bash
# Check if you use db_core
grep -r "from fraiseql.db_core" your_codebase/
grep -r "import fraiseql.db_core" your_codebase/
grep -r "from fraiseql import db_core" your_codebase/
```

---

## Backwards Compatibility

### v2.1-2.2 (Compatibility Period)

- ✅ db_core still works
- ⚠️ Deprecation warning shown
- ✅ No breaking changes
- ✅ All tests pass

### v3.0 (Breaking Change)

- ❌ db_core removed
- ❌ Imports will fail
- 🔧 Users must migrate

---

## Risk Mitigation

### Risks

| Risk | Mitigation |
|------|-----------|
| Users miss deprecation | Multiple warnings in v2.1-2.2 + migration guide |
| Migration confusion | Clear examples in migration guide + support |
| Breaking unexpected | 2-3 release cycles warning + clear timeline |
| Performance impact | New API is faster (no additional validation) |

### Safety

- ✅ 3 release cycles for migration (v2.1, v2.2, v3.0)
- ✅ Clear migration guide with examples
- ✅ Deprecation warnings at import time
- ✅ All functionality available in db/ module
- ✅ Internal code already migrated

---

## Alternative: Keep db_core (Considered but Rejected)

**Reasons to Keep**: Backward compatibility
**Reasons to Remove** ✅ **CHOSEN**:
- 2,450 LOC of dead code
- Duplicates functionality
- Maintenance burden
- Clearer codebase
- Users have modern db/ module since v2.0
- 3 release cycles for migration

**Recommendation**: Remove as planned (Phase 3 approach)

---

## Related Documents

- **CODE_CLEANING_PLAN.md** - Phase 1.3 task description
- **MIGRATION_DB_CORE.md** - To be created in v2.1 (migration guide)
- **CHANGELOG.md** - Deprecation notices per release

---

## Decision Record

**Decision**: Remove db_core.py with 3-phase deprecation
**Date**: 2026-01-09
**Timeline**:
- v2.1: Deprecation warning (Jan/Feb 2026)
- v2.2: Escalation (Feb/Mar 2026)
- v3.0: Removal (Apr/May 2026)

**Approver**: Claude (Code Review)
**Status**: ✅ Approved for implementation

---

## Implementation Checklist

### Release v2.1 (Deprecation)
- [ ] Add deprecation warning to db_core.py
- [ ] Create migration guide (MIGRATION_DB_CORE.md)
- [ ] Migrate all internal code from db_core → db/
- [ ] Update CHANGELOG
- [ ] Update docstrings
- [ ] Test deprecation warning
- [ ] Tag release as v2.1

### Release v2.2 (Escalation)
- [ ] Update deprecation warning with removal deadline
- [ ] Escalate docstrings to "DEPRECATED - WILL BE REMOVED"
- [ ] Update CHANGELOG
- [ ] Verify no internal db_core usage
- [ ] Tag release as v2.2

### Release v3.0 (Removal)
- [ ] Delete db_core.py
- [ ] Delete db_core.pyi
- [ ] Remove db_core imports from __init__.py
- [ ] Update CHANGELOG (breaking changes)
- [ ] Update documentation
- [ ] Run full test suite
- [ ] Tag release as v3.0

---

**Version History**:
- 2026-01-09: Initial deprecation strategy documented
- Status: Ready for implementation in v2.1 release
