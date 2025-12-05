# Archived: Dual-Mode System Tests

**Archived Date**: 2025-10-22
**Reason**: Feature removed - dual-mode system no longer exists
**Tests Removed**: 11 skipped tests

---

## Why Archived?

FraiseQL previously had a "dual-mode" system that supported two execution modes:
- **Development mode**: Python-based query execution with full object instantiation
- **Production mode**: Rust pipeline with zero-copy HTTP response generation

As of v0.11.x, the Rust pipeline is now **always used** for optimal performance. The development mode and all its infrastructure have been removed, making these tests obsolete.

---

## What Was Tested?

The archived `test_dual_mode_repository_unit.py` tested:

1. **Mode detection** from environment variables (`FRAISEQL_ENV`)
2. **Mode override** from context parameters
3. **Recursive object instantiation** in development mode
4. **Nested object handling** with Python types
5. **Circular reference protection** in object graphs
6. **CamelCase to snake_case conversion** in development mode
7. **Type extraction** from Optional and List types
8. **Query building** with parameter embedding

---

## Migration Notes

If you need similar functionality:

### For Mode Detection
The Rust pipeline is now always active. No mode detection needed.

### For Object Instantiation
The Rust pipeline returns `RustResponseBytes` directly. If you need Python objects:
```python
# Modern approach (v0.11.x+)
import json
from fraiseql.core.rust_pipeline import RustResponseBytes

result = await repo.find("my_view")
if isinstance(result, RustResponseBytes):
    json_str = bytes(result).decode("utf-8")
    data = json.loads(json_str)
    # Work with raw dict/list data
```

### For Type Safety
Use GraphQL types directly in your schema instead of relying on Python object instantiation:
```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
```

---

## Related Documentation

- Rust pipeline: `docs/rust/README.md`
- Performance guide: `docs/performance/PERFORMANCE_GUIDE.md`

---

## Archived File

The test file has been renamed to prevent pytest from collecting it:
- **Original name**: `test_dual_mode_repository_unit.py`
- **Archived name**: `dual_mode_repository_unit.py.archived`

This ensures the tests don't show up in test runs while keeping the code available for reference.

## Restoration

If you need to restore these tests for reference:
```bash
# View from git history
git show HEAD~1:tests/integration/database/repository/test_dual_mode_repository_unit.py

# Or view the archived file directly
cat tests/archived_tests/dual_mode_system/dual_mode_repository_unit.py.archived
```

**Note**: Do not restore for active testing - the dual-mode system no longer exists.
