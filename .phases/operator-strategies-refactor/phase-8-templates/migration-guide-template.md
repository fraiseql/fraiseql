# Migrating from operator_strategies to operators

## Summary

**Version:** v1.0.0+ (Phase 7 complete)

**Breaking Change:** The monolithic `fraiseql.sql.operator_strategies` module has been replaced with a modular `fraiseql.sql.operators` architecture.

**Impact:**
- **Internal FraiseQL code:** Already migrated (Phase 7)
- **FraiseQL contributors:** Update imports when contributing
- **FraiseQL library users:** Only if directly importing operator strategies (uncommon)

**Migration Time:** 5-15 minutes for most codebases

---

## Quick Migration Guide

### For Contributors (Most Common)

**OLD:**
```python
from fraiseql.sql.operator_strategies import (
    BaseOperatorStrategy,
    NetworkOperatorStrategy,
    StringOperatorStrategy,
)
```

**NEW:**
```python
from fraiseql.sql.operators import (
    BaseOperatorStrategy,
    NetworkOperatorStrategy,
    StringOperatorStrategy,
)
```

**Change:** Replace `operator_strategies` → `operators` in imports

---

## Migration Steps

### Step 1: Find All Imports

```bash
# Search your codebase
grep -r "from.*operator_strategies import\|import.*operator_strategies" . --include="*.py"
```

### Step 2: Update Imports

For each file found:

```bash
# Automated replacement (backup first!)
sed -i.bak 's/from fraiseql\.sql\.operator_strategies import/from fraiseql.sql.operators import/g' <file>
```

Or manually update imports:

```python
# Before
from fraiseql.sql.operator_strategies import BaseOperatorStrategy

# After
from fraiseql.sql.operators import BaseOperatorStrategy
```

### Step 3: Update API Usage (if applicable)

If using old API patterns, update to new registry-based API:

```python
# OLD (still works in new module)
from fraiseql.sql.operators import NetworkOperatorStrategy
strategy = NetworkOperatorStrategy()
sql = strategy.build_sql("isprivate", True, path_sql)

# NEW (preferred)
from fraiseql.sql.operators import get_default_registry
registry = get_default_registry()
sql = registry.build_sql("isprivate", True, path_sql, field_type=IPv4Address)
```

### Step 4: Run Tests

```bash
# Run your tests to verify migration
pytest tests/

# Should pass with no ModuleNotFoundError
```

---

## Common Migration Issues

### Issue 1: ModuleNotFoundError

**Error:**
```
ModuleNotFoundError: No module named 'fraiseql.sql.operator_strategies'
```

**Solution:**
```python
# Change:
from fraiseql.sql.operator_strategies import X

# To:
from fraiseql.sql.operators import X
```

### Issue 2: Import AttributeError

**Error:**
```
AttributeError: module 'fraiseql.sql.operators' has no attribute 'build_operator_sql'
```

**Solution:**
Old helper functions don't exist in new module. Use registry pattern:

```python
# OLD (doesn't exist):
from fraiseql.sql.operators import build_operator_sql

# NEW (use registry):
from fraiseql.sql.operators import get_default_registry
registry = get_default_registry()
```

---

## Migration Checklist

- [ ] Find all imports of `operator_strategies` in your code
- [ ] Replace `from fraiseql.sql.operator_strategies` with `from fraiseql.sql.operators`
- [ ] Update any old API usage to new registry-based API
- [ ] Run tests to verify migration
- [ ] Update documentation/comments referencing old module
- [ ] Commit changes

---

## Need Help?

- **Architecture docs:** `docs/architecture/operator-strategies.md`
- **Developer guide:** `docs/guides/adding-custom-operators.md`
- **API reference:** `docs/reference/operator-api.md`
- **Examples:** `docs/examples/operator-usage.md`
- **GitHub issues:** [Create an issue](https://github.com/yourorg/fraiseql/issues)
