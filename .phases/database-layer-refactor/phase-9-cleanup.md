# Phase 9: Legacy Cleanup

**Phase:** CLEANUP (Remove Deprecated Code)
**Duration:** 2-3 hours
**Risk:** Low (with feature flag)

---

## Objective

**TDD Phase CLEANUP:** Remove old db.py, finalize API.

Remove:
- Original `src/fraiseql/db.py` (2,078 lines)
- Backward compatibility shims
- Deprecation warnings
- Dead code

Update:
- All imports across codebase
- Error messages
- Documentation references

---

## Implementation with Feature Flag

Add environment variable for gradual rollout:

```python
# fraiseql/db/__init__.py
import os

USE_NEW_DB_LAYER = os.getenv("FRAISEQL_USE_NEW_DB", "true").lower() == "true"

if USE_NEW_DB_LAYER:
    from .repository import FraiseQLRepository
else:
    # Fallback to old implementation (for emergency rollback)
    import warnings
    warnings.warn("Using legacy db.py - deprecated")
    from ..db_legacy import FraiseQLRepository
```

---

## Rollback Plan

1. Set `FRAISEQL_USE_NEW_DB=false`
2. Revert to old db.py (kept as db_legacy.py)
3. Fix issues
4. Re-enable new layer

---

## Next Phase

→ **Phase 10:** Documentation
