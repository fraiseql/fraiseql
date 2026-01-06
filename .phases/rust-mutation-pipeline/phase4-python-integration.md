# Phase 4: Python Integration

**Duration**: 2 days
**Objective**: Simplify Python code to use new Rust pipeline, delete obsolete code
**Status**: NOT STARTED

**Prerequisites**: Phase 3 complete (Rust pipeline working, PyO3 bindings exposed)

## Overview

**CRITICAL**: This phase WILL break existing tests temporarily. We're changing from typed objects to dicts.

Tasks:
1. Simplify `rust_executor.py` (remove entity flattening)
2. Update `mutation_decorator.py` (remove parsing, return dicts)
3. Delete obsolete files (`entity_flattener.py`, `parser.py`)
4. Update tests for new behavior

## Tasks

### Task 4.1: Simplify rust_executor.py

**File**: `src/fraiseql/mutations/rust_executor.py` (UPDATE)

**Objective**: Remove entity flattening, simplify to just call Rust

**Changes**:
- ❌ Remove `flatten_entity_wrapper()` import and call
- ❌ Remove format conversion logic
- ✅ Call `fraiseql_rs.build_mutation_response()` directly

**Implementation**:

See `/tmp/fraiseql_rust_greenfield_implementation_plan_v2.md` lines 1863-2001 for full code.

**Key changes**:
```python
# BEFORE (lines to delete)
from fraiseql.mutations.entity_flattener import flatten_entity_wrapper

# ... later in code ...
flattened_entity = flatten_entity_wrapper(
    mutation_result, success_type_class, ...
)

# AFTER (simplified)
# Just call Rust directly - no flattening needed
response_bytes = fraiseql_rs.build_mutation_response(
    mutation_json,
    field_name,
    success_type,
    error_type,
    entity_field_name,
    entity_type,
    cascade_selections,
    auto_camel_case,
    success_type_fields,
)
return RustResponseBytes(response_bytes, schema_type=success_type)
```

**Acceptance Criteria**:
- [ ] File ~50 lines shorter
- [ ] No flattening logic
- [ ] No format conversion
- [ ] Calls Rust directly
- [ ] Returns `RustResponseBytes`

---

### Task 4.2: Update mutation_decorator.py

**File**: `src/fraiseql/mutations/mutation_decorator.py` (UPDATE)

**Objective**: Remove Python parsing, return dicts instead of typed objects

**Changes**:
- ❌ Remove `parse_mutation_result()` import and call
- ❌ Remove `__cascade__` attribute attachment
- ✅ Return dicts directly in non-HTTP mode

**Implementation**:

Find this section (around lines 268-326) and replace:

```python
# BEFORE (DELETE THIS)
from fraiseql.mutations.parser import parse_mutation_result

# ... later in resolver ...
parsed_result = parse_mutation_result(
    rust_response.to_json(),
    success_type_class,
    error_type_class,
)

# Attach CASCADE
if hasattr(parsed_result, '__dict__'):
    parsed_result.__cascade__ = cascade_data

return parsed_result

# AFTER (NEW CODE)
# Check if we're in HTTP mode
http_mode = info.context.get("_http_mode", False)

if http_mode:
    # HTTP PATH: Return RustResponseBytes directly to HTTP
    # PostgreSQL → Rust → HTTP bytes (zero Python parsing)
    return rust_response

# NON-HTTP PATH: Convert to dict for GraphQL execute()
# Used in tests and direct GraphQL execute() calls
try:
    graphql_response = rust_response.to_json()
    mutation_result = graphql_response["data"][field_name]
    logger.debug(f"Parsed GraphQL response for field '{field_name}'")
except Exception as e:
    logger.error(
        f"Failed to parse GraphQL response for mutation {self.name}",
        extra={"field_name": field_name, "error": str(e)},
    )
    raise

# Return dict directly (no parsing into Python objects)
# CASCADE is already at correct level from Rust
# Tests will work with dict access: result["user"]["id"]
return mutation_result
```

**Acceptance Criteria**:
- [ ] No Python parsing
- [ ] Returns `RustResponseBytes` in HTTP mode
- [ ] Returns dict in non-HTTP mode
- [ ] No `__cascade__` attribute attachment
- [ ] CASCADE already in correct place from Rust

---

### Task 4.3: Delete Obsolete Files

**Files to delete**:
1. `src/fraiseql/mutations/entity_flattener.py`
2. `src/fraiseql/mutations/parser.py`
3. `tests/unit/mutations/test_entity_flattener.py`

**Verification**:
```bash
# Delete files
rm src/fraiseql/mutations/entity_flattener.py
rm src/fraiseql/mutations/parser.py
rm tests/unit/mutations/test_entity_flattener.py

# Check for remaining imports (should be none)
grep -r "entity_flattener" src/
grep -r "mutations.parser" src/

# If any found, remove those imports
```

**Check for other files importing these**:
```bash
# Find all imports
git grep "from fraiseql.mutations.entity_flattener"
git grep "from fraiseql.mutations.parser"

# Remove any found imports
```

**Acceptance Criteria**:
- [ ] Files deleted (~700 LOC removed)
- [ ] No remaining imports
- [ ] No import errors when running tests

---

### Task 4.4: Update Tests for Dict Responses

**Files to update**:

1. **`tests/unit/mutations/test_rust_executor.py`**

Change from:
```python
# OLD
response = result.to_json()
assert response["data"]["createUser"]["user"]["__typename"] == "User"
```

To:
```python
# NEW (same - already using dicts)
response = result.to_json()
assert response["data"]["createUser"]["user"]["__typename"] == "User"
```

**This file should mostly work as-is** - it's already testing dict access!

2. **`tests/integration/graphql/mutations/test_mutation_patterns.py`**

Update any tests that expect typed objects:

```python
# OLD (if any exist)
assert result.user.id == "123"

# NEW
assert result["user"]["id"] == "123"
```

3. **Create new integration test for dict responses**

**File**: `tests/integration/graphql/mutations/test_mutation_dict_responses.py` (NEW)

```python
"""Test that mutations return dicts in non-HTTP mode."""

import pytest
from graphql import execute
from fraiseql.gql.schema_builder import build_schema


@pytest.mark.asyncio
async def test_mutation_returns_dict(db_pool):
    """Mutations should return dicts, not typed objects."""
    # This is a regression test for the Rust pipeline migration

    # Create simple mutation
    # ... setup schema ...

    result = await execute(
        schema,
        mutation_query,
        variable_values={"input": {"name": "Test"}},
    )

    # Result should be dict
    assert isinstance(result.data, dict)
    assert isinstance(result.data["createUser"], dict)

    # Can access with dict syntax
    assert result.data["createUser"]["user"]["id"] == "123"

    # CASCADE should be at success level
    if "cascade" in result.data["createUser"]:
        assert isinstance(result.data["createUser"]["cascade"], dict)
        # NOT in entity
        assert "cascade" not in result.data["createUser"]["user"]
```

**Acceptance Criteria**:
- [ ] All existing tests updated for dict access
- [ ] New integration test added
- [ ] Tests use dict syntax: `result["field"]["nested"]`
- [ ] No tests expecting typed objects remain

---

### Task 4.5: Update Test Fixtures (IF NEEDED)

Some tests may have fixtures that create typed response objects. Update these:

```python
# OLD
@pytest.fixture
def success_response():
    return UpdateUserSuccess(
        user=User(id="123", name="Test"),
        message="Updated"
    )

# NEW
@pytest.fixture
def success_response():
    return {
        "__typename": "UpdateUserSuccess",
        "user": {
            "__typename": "User",
            "id": "123",
            "name": "Test"
        },
        "message": "Updated"
    }
```

**Acceptance Criteria**:
- [ ] Fixtures return dicts
- [ ] Fixtures have correct structure with __typename
- [ ] Tests using fixtures updated

---

## Phase 4 Completion Checklist

- [ ] Task 4.1: `rust_executor.py` simplified
- [ ] Task 4.2: `mutation_decorator.py` updated
- [ ] Task 4.3: Obsolete files deleted
- [ ] Task 4.4: Tests updated for dict responses
- [ ] Task 4.5: Fixtures updated (if needed)
- [ ] All tests pass: `pytest tests/`
- [ ] No import errors
- [ ] ~700 LOC deleted
- [ ] Code coverage maintained (>85%)

**Verification**:
```bash
# Count deleted lines
git diff --stat

# Check for import errors
python3 -c "from fraiseql.mutations.rust_executor import execute_mutation_rust"
python3 -c "from fraiseql.mutations.mutation_decorator import mutation"

# Run all mutation tests
pytest tests/unit/mutations/ -v
pytest tests/integration/graphql/mutations/ -v

# Check coverage
pytest tests/ --cov=fraiseql.mutations --cov-report=term-missing
```

## Known Breaking Changes

**For users**: None - all changes are internal

**For tests**:
- Tests now receive dicts instead of typed objects
- Dict access: `result["user"]["id"]` instead of `result.user.id`
- CASCADE at success level: `result["cascade"]` not `result.user.cascade`

## Rollback Plan

If critical issues found:
1. Revert Phase 4 commits
2. Keep Phase 1-3 (Rust code is harmless if not called)
3. Fix issues
4. Retry Phase 4

## Next Phase

Once Phase 4 is complete and all tests pass, proceed to **Phase 5: Testing & Validation** for comprehensive edge case testing and property-based tests.
