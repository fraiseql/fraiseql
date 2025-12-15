# Phase 1: Preserve Type Annotations [REFACTOR]

**Objective**: Fix the `@connection` decorator to preserve type annotations on the wrapper function so the GraphQL schema builder can extract the correct arguments and return type.

**Context**: The `@wraps(func)` decorator preserves `__name__`, `__doc__`, etc., but NOT `__annotations__`. This causes `get_type_hints(wrapper)` to return an empty dict `{}`, so the schema builder can't determine the function's arguments or return type.

**Files to Modify**:
- `src/fraiseql/decorators.py` (lines 865-946)

**Priority**: P1 - Foundation phase
**Depends On**: Nothing
**Blocks**: Phase 2
**Can Run in Parallel**: No

---

## Implementation Plan

### Root Cause Analysis

The `@connection` decorator uses `@wraps(func)` to preserve function metadata, but `functools.wraps` does NOT preserve `__annotations__`. This means:

```python
@fraise_type(sql_source='users')
class User:
    id: int
    name: str

@connection(node_type=User)
async def users_connection(info, first: int | None = None, after: str | None = None) -> Connection[User]:
    return []

# After decoration:
print(get_type_hints(users_connection))  # {} - EMPTY!
```

The GraphQL schema builder calls `get_type_hints(wrapper)` to extract pagination arguments and return type, but gets an empty dict.

### Solution

1. **Import required types**:
   ```python
   from typing import get_type_hints
   from fraiseql.types.generic import Connection
   ```

2. **Construct wrapper_annotations dict** before `@wraps(func)`:
   ```python
   wrapper_annotations = {
       'info': GraphQLResolveInfo,
       'first': int | None,
       'after': str | None,
       'last': int | None,
       'before': str | None,
       'where': dict[str, Any] | None,
       'return': Connection[node_type],
   }
   ```

3. **Set annotations after wrapper creation**:
   ```python
   wrapper.__annotations__ = wrapper_annotations
   ```

### Verification

**Test Script**:
```python
from typing import get_type_hints
from fraiseql import fraise_type
from fraiseql.decorators import connection, query as query_decorator
from fraiseql.types.generic import Connection

@fraise_type(sql_source='users')
class User:
    id: int
    name: str

@query_decorator
@connection(node_type=User)
async def users_connection(info):
    return []

# Test: get_type_hints should return proper annotations
hints = get_type_hints(users_connection)
assert 'first' in hints
assert 'after' in hints
assert 'return' in hints
assert str(hints['return']).startswith('fraiseql.types.generic.Connection')
```

---

## Files to Modify

### `src/fraiseql/decorators.py`

**Lines 865-946**: The `@connection` decorator function

**Changes**:
1. Add imports: `get_type_hints`, `Connection`
2. Construct `wrapper_annotations` dict before `@wraps(func)`
3. Set `wrapper.__annotations__ = wrapper_annotations` after metadata assignment

---

## Testing Strategy

### Unit Test
- Create test script to verify `get_type_hints(wrapper)` returns correct annotations
- Verify pagination arguments: `first`, `after`, `last`, `before`, `where`
- Verify return type: `Connection[T]`

### Integration Test
- Run existing decorator tests to ensure no regressions
- Connection tests should remain skipped (Phase 2 required for schema generation)

### Verification Commands
```bash
# Test annotations preservation
python test_connection_annotations.py

# Test no regressions
uv run pytest tests/integration/meta/test_all_decorators.py -v
```

---

## Acceptance Criteria

- [ ] `get_type_hints(connection_wrapper)` returns proper annotations dict
- [ ] Pagination arguments (`first`, `after`, `last`, `before`, `where`) are present
- [ ] Return type is `Connection[T]` not `list[T]`
- [ ] Existing tests still pass
- [ ] No breaking changes to decorator API

---

## Commit Message

```
refactor(decorators): preserve type annotations on @connection wrapper [REFACTOR]

The @connection decorator now properly preserves type annotations on its
wrapper function, enabling GraphQL schema builder to extract pagination
arguments and Connection return type.

Root cause:
- @wraps(func) preserves __name__ and __doc__ but not __annotations__
- Schema builder calls get_type_hints() which returned {}
- GraphQL schema generation failed (no args, wrong return type)

Changes:
- Construct wrapper_annotations dict with pagination args and Connection return
- Set wrapper.__annotations__ after wrapper definition
- Add imports for Connection type and get_type_hints

Impact:
- Schema builder can now extract first/after/last/before/where arguments
- Return type correctly identified as Connection[T] not list[T]
- Enables Phase 2 (Connection type registration in schema)

Files modified:
- src/fraiseql/decorators.py: Added annotation preservation logic

Test:
- Created and ran test_connection_annotations.py (verify hints)
- Integration tests still pass (connection tests remain skipped until Phase 2)

Next: Phase 2 - Register Connection/Edge/PageInfo types in schema builder
```

---

## Rollback Plan

If issues arise:
```bash
git revert <commit-hash>
# Remove the annotation preservation code
# Tests will go back to being skipped
```
