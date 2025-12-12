# Phase 2: Implement Input Key Conversion

**Feature**: Input CamelCase → snake_case Conversion for PostgreSQL
**Phase**: 2/3 - Implement the utility function
**Type**: TDD GREEN Phase

---

## Objective

Implement `dict_keys_to_snake_case()` to make all tests from Phase 1 pass.

---

## Context

**Prerequisite**: Phase 1 completed (tests exist and are failing)

**Goal**: Write the implementation that satisfies all test cases, including:
- Simple and nested dicts
- Lists of dicts
- Deeply nested structures
- Edge cases (empty values, primitives, None)
- Acronyms and consecutive capitals

---

## Files to Modify

1. **`src/fraiseql/utils/casing.py`**: Replace `NotImplementedError` with actual implementation

---

## Implementation Steps

### Step 1: Implement the Function

**File**: `src/fraiseql/utils/casing.py`

Replace the placeholder with this implementation:

```python
def dict_keys_to_snake_case(data: dict | list | Any) -> dict | list | Any:
    """Recursively convert dictionary keys from camelCase to snake_case.

    This function is used to convert GraphQL input (camelCase) to PostgreSQL-compatible
    format (snake_case) before serializing to JSONB.

    Args:
        data: Input data structure (dict, list, or primitive)

    Returns:
        Data structure with all dict keys converted to snake_case

    Examples:
        >>> dict_keys_to_snake_case({"firstName": "John", "lastName": "Doe"})
        {'first_name': 'John', 'last_name': 'Doe'}

        >>> dict_keys_to_snake_case({"user": {"emailAddress": "john@example.com"}})
        {'user': {'email_address': 'john@example.com'}}

        >>> dict_keys_to_snake_case({"items": [{"itemName": "A"}, {"itemName": "B"}]})
        {'items': [{'item_name': 'A'}, {'item_name': 'B'}]}
    """
    if isinstance(data, dict):
        # Recursively convert all keys in the dict
        return {to_snake_case(key): dict_keys_to_snake_case(value) for key, value in data.items()}
    elif isinstance(data, list):
        # Recursively convert all items in the list
        return [dict_keys_to_snake_case(item) for item in data]
    else:
        # Primitive value - return as-is
        return data
```

**Rationale**:
- Uses existing `to_snake_case()` function from the same module (line 12 in `casing.py`)
- Recursively handles nested dicts and lists
- Preserves primitive values (str, int, UUID, date, None, etc.)
- Clean, functional implementation

### Step 2: Verify Implementation

Run tests to ensure all pass:

```bash
# Run the new tests
uv run pytest tests/unit/utils/test_dict_keys_to_snake_case.py -v

# Run all casing tests to ensure no regressions
uv run pytest tests/unit/utils/test_*camel*.py -v
```

**Expected Output**:
- ✅ All 15+ tests in `test_dict_keys_to_snake_case.py` pass
- ✅ No regressions in existing camelCase tests

### Step 3: Check Linting

```bash
uv run ruff check src/fraiseql/utils/casing.py
```

**Expected Output**: No issues

---

## Verification Commands

```bash
# Run unit tests
uv run pytest tests/unit/utils/test_dict_keys_to_snake_case.py -v

# Run all casing-related tests (regression check)
uv run pytest tests/unit/utils/test_*camel*.py -v

# Lint the modified file
uv run ruff check src/fraiseql/utils/casing.py
```

**Expected Output**:
- ✅ All new tests pass
- ✅ No regressions in existing tests
- ✅ Linting passes

---

## Acceptance Criteria

- [x] `dict_keys_to_snake_case()` implemented in `src/fraiseql/utils/casing.py`
- [x] All 15+ tests from Phase 1 pass
- [x] No regressions in existing camelCase tests
- [x] Linting passes
- [x] Function handles all edge cases:
  - Simple dicts
  - Nested dicts (multiple levels)
  - Lists of dicts
  - Lists of primitives
  - Mixed lists
  - Empty dicts/lists
  - None values
  - Acronyms (IP, DNS, HTTP)
  - Consecutive capitals

---

## DO NOT

- ❌ Modify `to_snake_case()` function (already exists and works)
- ❌ Change test expectations
- ❌ Skip edge case handling
- ❌ Add dependencies beyond standard library

---

## Notes

- Reuses existing `to_snake_case()` from line 12 of `casing.py`
- Simple recursive implementation (5 lines of logic)
- Functional programming style (no mutations)
- Handles all test cases without special-casing

---

## Next Phase

**Phase 3**: Integrate `dict_keys_to_snake_case()` into `rust_executor.py` before sending JSON to PostgreSQL.
