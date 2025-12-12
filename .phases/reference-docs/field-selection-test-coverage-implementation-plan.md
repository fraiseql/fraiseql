# Field Selection Test Coverage - Implementation Plan

**Document Version**: 1.0
**Date**: 2025-12-12
**Author**: CTO/Architect
**Status**: Ready for Execution
**Estimated Effort**: 6-8 hours
**Priority**: High
**Risk Level**: Low-Medium

---

## Executive Summary

### Current State

FraiseQL's field selection/filtering mechanism has **partial test coverage** with some outdated tests:

**Working Tests** (2 test files):
- ✅ `tests/unit/mutations/test_rust_field_selection.py` - **1/4 tests passing** (Rust layer)
- ✅ `tests/integration/graphql/mutations/test_selection_filter.py` - **All passing** (Python layer)

**Broken Tests** (1 test file):
- ❌ `tests/test_mutation_field_selection_integration.py` - **4/5 tests failing** (outdated expectations)

**Root Cause**: Tests were written before v1.8.1 changes and expect fields that no longer exist on certain types:
- Success types no longer have `errors` field (semantically removed)
- Error types no longer have `updatedFields` and `id` fields (v1.8.1 breaking change)
- Old Rust API signatures (`build_graphql_response` → `build_mutation_response`)

**Missing Coverage**:
- ❌ Comprehensive Error type field filtering tests
- ❌ Named fragment support (added in v1.8.1)
- ❌ Edge cases: empty selection, nested entities, cascade filtering
- ❌ E2E integration tests (GraphQL → Database → Rust → Response)
- ❌ Performance benchmarks

### Proposed State

After this implementation plan:
- ✅ All field selection tests passing (100%)
- ✅ Comprehensive coverage for Success AND Error type field filtering
- ✅ Named fragment support tested
- ✅ Edge cases covered
- ✅ E2E integration tests
- ✅ Performance benchmarks
- ✅ Clean, maintainable test organization

### Effort Estimate

| Phase | Description | Effort |
|-------|-------------|--------|
| Phase 0 | Assessment & Inventory | 0.5h |
| Phase 1 | Fix Outdated Tests (Quick Wins) | 1.5h |
| Phase 2 | Error Type Field Selection Tests | 1.5h |
| Phase 3 | Named Fragment Tests | 1h |
| Phase 4 | Edge Case Tests | 1.5h |
| Phase 5 | E2E Integration Tests | 1.5h |
| Phase 6 | Performance Benchmarks | 1h |
| Phase 7 | Documentation & Cleanup | 0.5h |
| **Total** | | **8-9h** |

### Risk Level: **Low-Medium**

**Low Risk**:
- Field selection logic is already working in production
- We're adding/fixing tests, not changing behavior
- No database schema changes

**Medium Risk**:
- Some tests use Rust API directly (could reveal API issues)
- E2E tests might expose integration bugs

**Mitigation**: Each phase is independent with its own verification. Can pause/rollback at any phase boundary.

---

## Test Coverage Matrix

| Scenario | Current Coverage | Target Coverage | Priority | Effort | Test File |
|----------|------------------|-----------------|----------|--------|-----------|
| **Success: Basic field filtering** | ✅ Partial | ✅ Complete | P0 | 0.5h | `test_rust_field_selection.py` |
| **Success: All fields requested** | ❌ Broken | ✅ Fixed | P0 | 0.5h | `test_rust_field_selection.py` |
| **Success: None selection (compat)** | ❌ Broken | ✅ Fixed | P0 | 0.5h | `test_rust_field_selection.py` |
| **Error: Field filtering** | ❌ Broken | ✅ Complete | P0 | 1.5h | `test_rust_field_selection.py` |
| **Error: Negative assertions** | ❌ Missing | ✅ Added | P1 | 0.5h | `test_rust_field_selection.py` |
| **Named fragments** | ❌ Missing | ✅ Complete | P1 | 1h | `test_named_fragments.py` (new) |
| **Empty selection** | ✅ Present | ✅ Enhanced | P2 | 0.5h | `test_selection_filter.py` |
| **Nested entity filtering** | ✅ Present | ✅ Enhanced | P2 | 0.5h | `test_selection_filter.py` |
| **Cascade field filtering** | ❌ Missing | ✅ Added | P2 | 1h | `test_rust_field_selection.py` |
| **Multiple entities (v1.8.1)** | ❌ Missing | ✅ Added | P2 | 1h | `test_multiple_entities_field_selection.py` (new) |
| **E2E: GraphQL → Rust** | ❌ Broken | ✅ Complete | P1 | 1.5h | `test_mutation_field_selection_integration.py` |
| **Performance benchmarks** | ❌ Missing | ✅ Added | P3 | 1h | `test_field_selection_performance.py` (new) |

**Priority Legend**:
- **P0**: Critical - blocking bugs, must fix immediately
- **P1**: High - important features, should fix soon
- **P2**: Medium - nice to have, can be deferred
- **P3**: Low - optimizations, future work

---

## Phase 0: Assessment & Inventory

**Objective**: Understand current state and verify FraiseQL version

**Duration**: 0.5 hours

### Implementation Steps

1. **Verify FraiseQL version and commit**
   ```bash
   cd /home/lionel/code/fraiseql
   git log --oneline -1
   # Expected: eaa1f78f or later (v1.8.1+)
   ```

2. **Run all field selection tests to get baseline**
   ```bash
   # Rust unit tests
   uv run pytest tests/unit/mutations/test_rust_field_selection.py -v

   # Python integration tests
   uv run pytest tests/integration/graphql/mutations/test_selection_filter.py -v

   # Old integration tests (expected to fail)
   uv run pytest tests/test_mutation_field_selection_integration.py -v
   ```

3. **Document current state**
   - Count passing vs failing tests
   - Identify exact failure reasons
   - List missing test coverage

4. **Review v1.8.1 CHANGELOG**
   - Confirm understanding of breaking changes
   - Note new features (named fragments, multiple entities)
   - Understand semantic changes (Success no `errors`, Error no `id`/`updatedFields`)

### Verification Commands

```bash
# All commands should complete without errors
cd /home/lionel/code/fraiseql
git status  # Should be clean or on feature branch
git log --oneline -1 | grep -E "(eaa1f78f|1b75b2ee|d40f142a)"  # v1.8.1+
uv run pytest tests/unit/mutations/test_rust_field_selection.py -v --tb=short | tee /tmp/phase0-rust-tests.log
uv run pytest tests/integration/graphql/mutations/test_selection_filter.py -v --tb=short | tee /tmp/phase0-integration-tests.log
```

### Acceptance Criteria

- [ ] FraiseQL version confirmed as v1.8.1+ (commit eaa1f78f or later)
- [ ] Baseline test results documented
- [ ] Failure reasons identified for each failing test
- [ ] Missing coverage documented in `/tmp/phase0-coverage-gaps.md`

### Deliverables

- `/tmp/phase0-rust-tests.log` - Rust unit test results
- `/tmp/phase0-integration-tests.log` - Integration test results
- `/tmp/phase0-coverage-gaps.md` - List of missing coverage

### Rollback Plan

No changes made in this phase - pure assessment.

---

## Phase 1: Fix Outdated Tests (Quick Wins)

**Objective**: Fix 3 failing tests in `test_rust_field_selection.py` by removing `errors` expectations on Success types

**Duration**: 1.5 hours

### Files to Modify

- `tests/unit/mutations/test_rust_field_selection.py`

### Implementation Steps

#### Step 1.1: Fix `test_rust_returns_all_fields_when_all_requested`

**Problem**: Line 86 requests `errors` field which doesn't exist on Success types (v1.9.0+)

**Fix**:
```python
# OLD (line 86):
selected_fields = ["status", "message", "errors", "updatedFields", "id", "machine"]

# NEW:
selected_fields = ["status", "message", "updatedFields", "id", "machine"]
```

**Fix assertions** (line 106):
```python
# OLD:
assert "errors" in data

# NEW:
# Remove this assertion - Success types don't have errors field
```

**Updated test** (lines 70-111):
```python
def test_rust_returns_all_fields_when_all_requested(fraiseql_rs):
    """Verify all fields returned when all are requested."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Request ALL Success-type fields (no 'errors' - not on Success types)
    selected_fields = ["status", "message", "updatedFields", "id", "machine"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # All requested fields should be present
    assert "status" in data
    assert "message" in data
    assert "updatedFields" in data
    assert "id" in data
    assert "machine" in data

    # Success types should NOT have errors field
    assert "errors" not in data, "Success types should not have errors field (v1.9.0+)"

    print(f"✅ All Success fields present when requested: {sorted(data.keys())}")
```

#### Step 1.2: Fix `test_rust_backward_compat_none_selection`

**Problem**: Line 148 expects `errors` field on Success type

**Fix**:
```python
# OLD (line 148):
assert "errors" in data, "errors should be present"

# NEW:
# Remove this assertion - Success types don't have errors field
assert "errors" not in data, "Success types should not have errors field (v1.9.0+)"
```

**Updated test** (lines 114-153):
```python
def test_rust_backward_compat_none_selection(fraiseql_rs):
    """Verify None selection returns all fields (backward compatibility)."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # No field selection (None) - should return ALL Success-type fields
    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        None,
        True,
        None,  # None = no filtering
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # All Success-type fields should be present
    assert "status" in data
    assert "message" in data
    assert "updatedFields" in data
    assert "id" in data
    assert "machine" in data

    # Success types should NOT have errors field
    assert "errors" not in data, "Success types don't have errors (v1.9.0+)"

    print("✅ Backward compat: None selection returns all Success fields")
```

#### Step 1.3: Fix `test_rust_error_response_field_filtering`

**Problem**: Test fixture creates a success response instead of an error response

**Root Cause Analysis**:
- Line 159: `status = "failed"` is correct for error
- Line 169: Has `errors` array (correct)
- Line 186: Requests `selected_fields = ["errors"]` (correct)
- **Issue**: Line 201 asserts `"code" not in data` but Error types HAVE `code` field (v1.8.1)

**Fix**: Update test to properly test Error response with v1.8.1 behavior

**Updated test** (lines 156-205):
```python
def test_rust_error_response_field_filtering(fraiseql_rs):
    """Verify error responses also respect field selection."""

    # Proper error response (v1.8.1)
    fake_error = {
        "status": "failed:validation",
        "message": "Validation error",
        "entity_id": None,
        "entity_type": None,
        "entity": None,
        "updated_fields": None,
        "cascade": None,
        "metadata": {
            "errors": [
                {"code": "VALIDATION_ERROR", "message": "Invalid input"}
            ]
        },
        "is_simple_format": False,
    }

    # Only request 'code' and 'errors' fields
    selected_fields = ["code", "errors"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_error),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have __typename
    assert "__typename" in data
    assert data["__typename"] == "CreateMachineError"

    # Should have requested fields
    assert "code" in data, "Error types have code field (v1.8.1)"
    assert data["code"] == 422  # failed:validation → 422
    assert "errors" in data
    assert len(data["errors"]) == 1

    # Should NOT have unrequested fields
    assert "message" not in data, f"message not requested, got keys: {list(data.keys())}"
    assert "status" not in data, "status not requested"

    # Error types should NOT have Success-only fields
    assert "id" not in data, "Error types don't have id field (v1.8.1)"
    assert "updatedFields" not in data, "Error types don't have updatedFields (v1.8.1)"

    print(f"✅ Error response filtering works: {list(data.keys())}")
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Run only the tests we fixed
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_rust_returns_all_fields_when_all_requested -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_rust_backward_compat_none_selection -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_rust_error_response_field_filtering -xvs

# Run entire file to ensure no regressions
uv run pytest tests/unit/mutations/test_rust_field_selection.py -v

# Expected: 4/4 tests passing
```

### Acceptance Criteria

- [ ] `test_rust_returns_all_fields_when_all_requested` - PASSING
- [ ] `test_rust_backward_compat_none_selection` - PASSING
- [ ] `test_rust_error_response_field_filtering` - PASSING
- [ ] `test_rust_filters_success_fields_correctly` - Still PASSING (no regression)
- [ ] All 4 tests in `test_rust_field_selection.py` passing (100%)

### Rollback Plan

```bash
git checkout tests/unit/mutations/test_rust_field_selection.py
```

---

## Phase 2: Error Type Field Selection Tests

**Objective**: Add comprehensive Error type field filtering tests

**Duration**: 1.5 hours

### Files to Modify

- `tests/unit/mutations/test_rust_field_selection.py` (add new tests)

### Implementation Steps

#### Step 2.1: Add test for Error type field filtering

**Add new test** (after line 205):
```python
def test_error_type_filters_auto_injected_fields(fraiseql_rs):
    """Verify Error types filter auto-injected fields (code, status, message, errors)."""

    fake_error = {
        "status": "failed:not_found",
        "message": "Machine not found",
        "entity_id": None,
        "entity_type": None,
        "entity": None,
        "updated_fields": None,
        "cascade": None,
        "metadata": {
            "errors": [
                {"code": "NOT_FOUND", "message": "No machine with ID 123"}
            ]
        },
        "is_simple_format": False,
    }

    # Only request 'code' field (not status, message, errors)
    selected_fields = ["code"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_error),
        "deleteMachine",
        "DeleteMachineSuccess",
        "DeleteMachineError",
        None,  # No entity field
        None,
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["deleteMachine"]

    # Should have __typename and requested field
    assert "__typename" in data
    assert data["__typename"] == "DeleteMachineError"
    assert "code" in data
    assert data["code"] == 404  # failed:not_found → 404

    # Should NOT have unrequested auto-injected fields
    assert "status" not in data, f"status not requested, got: {list(data.keys())}"
    assert "message" not in data, "message not requested"
    assert "errors" not in data, "errors not requested"

    print(f"✅ Error type filtering: only code present: {list(data.keys())}")


def test_error_type_all_auto_injected_fields(fraiseql_rs):
    """Verify Error types return all auto-injected fields when requested."""

    fake_error = {
        "status": "failed:conflict",
        "message": "Machine already exists",
        "entity_id": None,
        "entity_type": None,
        "entity": None,
        "updated_fields": None,
        "cascade": None,
        "metadata": {
            "errors": [
                {"code": "DUPLICATE", "message": "Serial number already exists"}
            ]
        },
        "is_simple_format": False,
    }

    # Request ALL Error-type auto-injected fields
    selected_fields = ["code", "status", "message", "errors"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_error),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # All auto-injected fields should be present
    assert "code" in data
    assert data["code"] == 409  # failed:conflict → 409
    assert "status" in data
    assert data["status"] == "failed:conflict"
    assert "message" in data
    assert data["message"] == "Machine already exists"
    assert "errors" in data
    assert len(data["errors"]) == 1

    print(f"✅ All Error auto-injected fields present: {sorted(data.keys())}")


def test_error_type_code_computation(fraiseql_rs):
    """Verify Error type 'code' field is computed correctly from status."""

    test_cases = [
        ("failed:validation", 422),
        ("failed:not_found", 404),
        ("failed:conflict", 409),
        ("noop:invalid_id", 422),
        ("failed:unknown", 500),
    ]

    for status, expected_code in test_cases:
        fake_error = {
            "status": status,
            "message": "Test error",
            "entity_id": None,
            "entity_type": None,
            "entity": None,
            "updated_fields": None,
            "cascade": None,
            "metadata": {"errors": []},
            "is_simple_format": False,
        }

        selected_fields = ["code"]

        response_json = fraiseql_rs.build_mutation_response(
            json.dumps(fake_error),
            "testMutation",
            "TestSuccess",
            "TestError",
            None,
            None,
            None,
            True,
            selected_fields,
        )

        response = json.loads(response_json)
        data = response["data"]["testMutation"]

        assert "code" in data
        assert data["code"] == expected_code, \
            f"Status '{status}' should map to {expected_code}, got {data['code']}"

    print("✅ Error code computation correct for all status types")
```

#### Step 2.2: Add negative assertion test (Error types don't have Success fields)

**Add new test**:
```python
def test_error_type_does_not_have_success_fields(fraiseql_rs):
    """Verify Error types never have Success-only fields (id, updatedFields)."""

    fake_error = {
        "status": "failed:validation",
        "message": "Validation failed",
        "entity_id": None,  # Even if present, should not appear
        "entity_type": None,
        "entity": None,
        "updated_fields": None,  # Even if present, should not appear
        "cascade": None,
        "metadata": {"errors": []},
        "is_simple_format": False,
    }

    # Request ALL fields including ones that shouldn't exist on Error types
    # This tests that Rust properly filters out semantically incorrect fields
    selected_fields = ["code", "status", "message", "errors", "id", "updatedFields"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_error),
        "testMutation",
        "TestSuccess",
        "TestError",
        None,
        None,
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["testMutation"]

    # Error-type fields should be present
    assert "code" in data
    assert "status" in data
    assert "message" in data
    assert "errors" in data

    # Success-only fields should NOT be present (even though requested)
    assert "id" not in data, "Error types should never have id field (v1.8.1)"
    assert "updatedFields" not in data, "Error types should never have updatedFields (v1.8.1)"

    print(f"✅ Error types correctly exclude Success-only fields: {list(data.keys())}")
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Run new tests
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_error_type_filters_auto_injected_fields -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_error_type_all_auto_injected_fields -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_error_type_code_computation -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_error_type_does_not_have_success_fields -xvs

# Run entire file
uv run pytest tests/unit/mutations/test_rust_field_selection.py -v

# Expected: 8/8 tests passing (4 from Phase 1 + 4 new)
```

### Acceptance Criteria

- [ ] `test_error_type_filters_auto_injected_fields` - PASSING
- [ ] `test_error_type_all_auto_injected_fields` - PASSING
- [ ] `test_error_type_code_computation` - PASSING
- [ ] `test_error_type_does_not_have_success_fields` - PASSING
- [ ] All previous tests still passing (no regressions)
- [ ] Total: 8/8 tests passing

### Rollback Plan

```bash
git diff tests/unit/mutations/test_rust_field_selection.py > /tmp/phase2.patch
git checkout tests/unit/mutations/test_rust_field_selection.py
# To restore: git apply /tmp/phase2.patch
```

---

## Phase 3: Named Fragment Tests

**Objective**: Add tests for named fragment support in field extraction

**Duration**: 1 hour

### Files to Create

- `tests/unit/mutations/test_named_fragments.py` (new file)

### Implementation Steps

#### Step 3.1: Create named fragment test file

**Create file**: `tests/unit/mutations/test_named_fragments.py`

```python
"""Test named fragment support in field selection."""
import json
import pytest
from fraiseql import _get_fraiseql_rs
from unittest.mock import MagicMock
from graphql import FieldNode, FragmentDefinitionNode, NameNode


@pytest.fixture
def fraiseql_rs():
    """Get Rust module."""
    return _get_fraiseql_rs()


def test_named_fragment_field_extraction():
    """Verify field extraction works with named fragments."""
    from fraiseql.mutations.mutation_decorator import _extract_selected_fields

    # Mock GraphQL info with named fragment
    mock_info = MagicMock()
    mock_field_node = MagicMock()

    # Create named fragment spread: ...MachineFields
    mock_fragment_spread = MagicMock()
    mock_fragment_spread.name.value = "MachineFields"

    # Create fragment definition
    mock_fragment_def = MagicMock(spec=FragmentDefinitionNode)
    mock_fragment_def.type_condition.name.value = "CreateMachineSuccess"

    # Fragment selections: status, message, machine { id name }
    mock_status = MagicMock(spec=FieldNode)
    mock_status.name.value = "status"
    mock_status.selection_set = None

    mock_message = MagicMock(spec=FieldNode)
    mock_message.name.value = "message"
    mock_message.selection_set = None

    mock_machine_id = MagicMock(spec=FieldNode)
    mock_machine_id.name.value = "id"
    mock_machine_id.selection_set = None

    mock_machine_name = MagicMock(spec=FieldNode)
    mock_machine_name.name.value = "name"
    mock_machine_name.selection_set = None

    mock_machine = MagicMock(spec=FieldNode)
    mock_machine.name.value = "machine"
    mock_machine.selection_set.selections = [mock_machine_id, mock_machine_name]

    mock_fragment_def.selection_set.selections = [mock_status, mock_message, mock_machine]

    # Set up info
    mock_field_node.selection_set.selections = [mock_fragment_spread]
    mock_info.field_nodes = [mock_field_node]
    mock_info.fragments = {"MachineFields": mock_fragment_def}

    # Extract fields
    selected_fields = _extract_selected_fields(mock_info)

    # Should extract fields from named fragment
    assert "status" in selected_fields
    assert "message" in selected_fields
    assert "machine" in selected_fields

    # Should NOT extract auto-injected fields that weren't requested
    assert "id" not in selected_fields, "id is top-level auto-injected, not in fragment"
    assert "updatedFields" not in selected_fields
    assert "errors" not in selected_fields

    print(f"✅ Named fragment extraction: {selected_fields}")


def test_named_fragment_with_inline_fragments():
    """Verify field extraction works with both named and inline fragments."""
    from fraiseql.mutations.mutation_decorator import _extract_selected_fields

    mock_info = MagicMock()
    mock_field_node = MagicMock()

    # Inline fragment: ... on CreateMachineSuccess { status }
    mock_inline_fragment = MagicMock()
    mock_inline_fragment.type_condition.name.value = "CreateMachineSuccess"

    mock_status = MagicMock(spec=FieldNode)
    mock_status.name.value = "status"
    mock_status.selection_set = None

    mock_inline_fragment.selection_set.selections = [mock_status]

    # Named fragment spread: ...MachineFields
    mock_fragment_spread = MagicMock()
    mock_fragment_spread.name.value = "MachineFields"

    mock_fragment_def = MagicMock(spec=FragmentDefinitionNode)
    mock_fragment_def.type_condition.name.value = "CreateMachineSuccess"

    mock_machine = MagicMock(spec=FieldNode)
    mock_machine.name.value = "machine"
    mock_machine.selection_set = None

    mock_fragment_def.selection_set.selections = [mock_machine]

    # Set up info with both inline and named fragments
    mock_field_node.selection_set.selections = [mock_inline_fragment, mock_fragment_spread]
    mock_info.field_nodes = [mock_field_node]
    mock_info.fragments = {"MachineFields": mock_fragment_def}

    selected_fields = _extract_selected_fields(mock_info)

    # Should extract from both inline and named fragments
    assert "status" in selected_fields  # From inline fragment
    assert "machine" in selected_fields  # From named fragment

    print(f"✅ Mixed fragments extraction: {selected_fields}")


def test_rust_with_named_fragment_fields(fraiseql_rs):
    """Verify Rust layer respects field selection from named fragments."""

    fake_result = {
        "status": "success",
        "message": "Machine created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test Machine"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Simulate fields extracted from named fragment
    # Fragment only requested: status, machine
    selected_fields = ["status", "machine"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should only have fields from fragment
    assert "status" in data
    assert "machine" in data

    # Should NOT have unrequested fields
    assert "message" not in data, "message not in fragment"
    assert "id" not in data, "id not in fragment"
    assert "updatedFields" not in data, "updatedFields not in fragment"

    print(f"✅ Rust respects named fragment selection: {list(data.keys())}")


def test_empty_named_fragment():
    """Verify empty named fragment doesn't crash field extraction."""
    from fraiseql.mutations.mutation_decorator import _extract_selected_fields

    mock_info = MagicMock()
    mock_field_node = MagicMock()

    # Named fragment with no selections
    mock_fragment_spread = MagicMock()
    mock_fragment_spread.name.value = "EmptyFragment"

    mock_fragment_def = MagicMock(spec=FragmentDefinitionNode)
    mock_fragment_def.type_condition.name.value = "CreateMachineSuccess"
    mock_fragment_def.selection_set.selections = []

    mock_field_node.selection_set.selections = [mock_fragment_spread]
    mock_info.field_nodes = [mock_field_node]
    mock_info.fragments = {"EmptyFragment": mock_fragment_def}

    selected_fields = _extract_selected_fields(mock_info)

    # Should return empty set (not crash)
    assert selected_fields == set() or len(selected_fields) == 0

    print("✅ Empty named fragment handled gracefully")


def test_missing_named_fragment():
    """Verify missing named fragment doesn't crash field extraction."""
    from fraiseql.mutations.mutation_decorator import _extract_selected_fields

    mock_info = MagicMock()
    mock_field_node = MagicMock()

    # Reference to non-existent fragment
    mock_fragment_spread = MagicMock()
    mock_fragment_spread.name.value = "MissingFragment"

    mock_field_node.selection_set.selections = [mock_fragment_spread]
    mock_info.field_nodes = [mock_field_node]
    mock_info.fragments = {}  # No fragments defined

    selected_fields = _extract_selected_fields(mock_info)

    # Should handle gracefully (return empty or continue)
    # Exact behavior depends on implementation
    assert isinstance(selected_fields, set)

    print("✅ Missing named fragment handled gracefully")
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Run new named fragment tests
uv run pytest tests/unit/mutations/test_named_fragments.py -v

# Expected: 6/6 tests passing
```

### Acceptance Criteria

- [ ] `test_named_fragment_field_extraction` - PASSING
- [ ] `test_named_fragment_with_inline_fragments` - PASSING
- [ ] `test_rust_with_named_fragment_fields` - PASSING
- [ ] `test_empty_named_fragment` - PASSING
- [ ] `test_missing_named_fragment` - PASSING
- [ ] Total: 6/6 tests passing in new file

### Rollback Plan

```bash
rm tests/unit/mutations/test_named_fragments.py
```

---

## Phase 4: Edge Case Tests

**Objective**: Add tests for edge cases (cascade, multiple entities, nested filtering)

**Duration**: 1.5 hours

### Files to Modify

- `tests/unit/mutations/test_rust_field_selection.py` (add edge case tests)

### Implementation Steps

#### Step 4.1: Add cascade field selection test

**Add new test**:
```python
def test_cascade_field_selection(fraiseql_rs):
    """Verify cascade field respects field selection."""

    fake_result = {
        "status": "success",
        "message": "Machine created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test Machine"},
        "updated_fields": ["name"],
        "cascade": {
            "deleted": {
                "Reservation": [
                    {"id": "r1", "name": "Reservation 1", "status": "cancelled"},
                    {"id": "r2", "name": "Reservation 2", "status": "cancelled"}
                ]
            },
            "updated": {}
        },
        "metadata": None,
        "is_simple_format": False,
    }

    # Request cascade but only select specific fields from cascade entities
    selected_fields = ["status", "machine", "cascade"]
    cascade_selections = {
        "Reservation": ["id", "status"]  # Only want id and status, not name
    }

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        json.dumps(cascade_selections),  # Pass cascade selections
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have cascade field
    assert "cascade" in data
    assert "deleted" in data["cascade"]
    assert "Reservation" in data["cascade"]["deleted"]

    # Cascade entities should be filtered
    reservations = data["cascade"]["deleted"]["Reservation"]
    assert len(reservations) == 2

    for reservation in reservations:
        assert "id" in reservation
        assert "status" in reservation
        assert "name" not in reservation, "name not requested in cascade selections"

    print(f"✅ Cascade field selection works: {list(reservations[0].keys())}")


def test_empty_cascade_selection(fraiseql_rs):
    """Verify empty cascade field selection returns only __typename."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": {
            "deleted": {"Reservation": [{"id": "r1", "name": "R1"}]},
            "updated": {}
        },
        "metadata": None,
        "is_simple_format": False,
    }

    # Request cascade but with empty selections
    selected_fields = ["cascade"]
    cascade_selections = {
        "Reservation": []  # Empty selection
    }

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        json.dumps(cascade_selections),
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have cascade but entities should be minimal
    assert "cascade" in data
    reservations = data["cascade"]["deleted"]["Reservation"]

    # With empty selection, should only have __typename
    for reservation in reservations:
        assert "__typename" in reservation
        # May or may not have other fields depending on Rust implementation

    print(f"✅ Empty cascade selection: {list(reservations[0].keys())}")
```

#### Step 4.2: Add multiple entity fields test

**Add new test**:
```python
def test_multiple_entity_fields_selection(fraiseql_rs):
    """Verify field selection with multiple entity fields (v1.8.1 feature)."""

    # Error response with conflict entity
    fake_error = {
        "status": "failed:conflict",
        "message": "Machine with this serial number already exists",
        "entity_id": None,
        "entity_type": None,
        "entity": {
            "conflict_machine": {
                "id": "existing-123",
                "name": "Existing Machine",
                "serial_number": "SN-001",
                "location": "Warehouse A"
            }
        },
        "updated_fields": None,
        "cascade": None,
        "metadata": {
            "errors": [
                {"code": "DUPLICATE_SERIAL", "message": "Serial number SN-001 already in use"}
            ]
        },
        "is_simple_format": False,
    }

    # Request code, conflictMachine (but only select specific fields from entity)
    selected_fields = ["code", "message", "conflictMachine"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_error),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",  # Primary entity field name
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have requested fields
    assert "code" in data
    assert data["code"] == 409
    assert "message" in data
    assert "conflictMachine" in data

    # Conflict entity should be present
    conflict = data["conflictMachine"]
    assert conflict["id"] == "existing-123"
    assert conflict["name"] == "Existing Machine"

    # Should NOT have unrequested Error fields
    assert "status" not in data, "status not requested"
    assert "errors" not in data, "errors not requested"

    print(f"✅ Multiple entity fields work: {list(data.keys())}")


def test_multiple_entity_fields_success_type(fraiseql_rs):
    """Verify field selection with multiple entities in Success type."""

    fake_result = {
        "status": "updated",
        "message": "Location updated",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {
            "machine": {"id": "123", "name": "Machine X"},
            "previous_location": {"id": "loc1", "name": "Warehouse A"},
            "new_location": {"id": "loc2", "name": "Warehouse B"}
        },
        "updated_fields": ["location_id"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Request machine and locations
    selected_fields = ["machine", "previousLocation", "newLocation"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "updateMachineLocation",
        "UpdateMachineLocationSuccess",
        "UpdateMachineLocationError",
        "machine",  # Primary entity
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["updateMachineLocation"]

    # Should have all requested entity fields
    assert "machine" in data
    assert data["machine"]["id"] == "123"
    assert "previousLocation" in data
    assert data["previousLocation"]["id"] == "loc1"
    assert "newLocation" in data
    assert data["newLocation"]["id"] == "loc2"

    # Should NOT have unrequested auto-injected fields
    assert "status" not in data
    assert "message" not in data
    assert "updatedFields" not in data
    assert "id" not in data

    print(f"✅ Multiple entities in Success: {list(data.keys())}")
```

#### Step 4.3: Add nested entity field selection test

**Add new test**:
```python
def test_nested_entity_field_selection(fraiseql_rs):
    """Verify nested entity fields respect selection (e.g., machine.contract.customer)."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {
            "id": "123",
            "name": "Machine X",
            "serial_number": "SN-001",
            "contract": {
                "id": "c1",
                "name": "Contract 1",
                "start_date": "2025-01-01",
                "customer": {
                    "id": "cust1",
                    "name": "Customer A",
                    "email": "customer@example.com",
                    "phone": "123-456-7890"
                }
            }
        },
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Request machine with only specific nested fields
    # Note: Rust layer doesn't currently support nested field selection within entities
    # This test documents current behavior
    selected_fields = ["machine"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        "machine",
        "Machine",
        None,
        True,
        selected_fields,
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have machine field
    assert "machine" in data
    machine = data["machine"]

    # Machine entity should have all its fields (no sub-field filtering currently)
    assert "id" in machine
    assert "name" in machine
    assert "contract" in machine

    # Nested contract should be present
    assert "customer" in machine["contract"]

    print(f"✅ Nested entities present (no sub-field filtering): {list(machine.keys())}")
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Run new edge case tests
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_cascade_field_selection -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_empty_cascade_selection -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_multiple_entity_fields_selection -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_multiple_entity_fields_success_type -xvs
uv run pytest tests/unit/mutations/test_rust_field_selection.py::test_nested_entity_field_selection -xvs

# Run entire file
uv run pytest tests/unit/mutations/test_rust_field_selection.py -v

# Expected: 13/13 tests passing (8 from earlier + 5 new)
```

### Acceptance Criteria

- [ ] `test_cascade_field_selection` - PASSING
- [ ] `test_empty_cascade_selection` - PASSING
- [ ] `test_multiple_entity_fields_selection` - PASSING
- [ ] `test_multiple_entity_fields_success_type` - PASSING
- [ ] `test_nested_entity_field_selection` - PASSING
- [ ] All previous tests still passing
- [ ] Total: 13/13 tests passing

### Rollback Plan

```bash
git diff tests/unit/mutations/test_rust_field_selection.py > /tmp/phase4.patch
git checkout tests/unit/mutations/test_rust_field_selection.py
git apply /tmp/phase1.patch  # Restore Phase 1 changes
git apply /tmp/phase2.patch  # Restore Phase 2 changes
```

---

## Phase 5: E2E Integration Tests

**Objective**: Fix/rewrite E2E integration tests in `test_mutation_field_selection_integration.py`

**Duration**: 1.5 hours

### Files to Modify

- `tests/test_mutation_field_selection_integration.py`

### Implementation Steps

#### Step 5.1: Fix decorator test (remove `errors` from Success types)

**Fix test** (lines 6-23):
```python
def test_decorator_adds_fields_to_gql_fields():
    """Verify Python decorator adds auto-populated fields to __gql_fields__."""

    @success
    class TestSuccess:
        entity: dict

    gql_fields = getattr(TestSuccess, "__gql_fields__", {})

    # Expected fields on Success types
    assert "entity" in gql_fields, "Original field should be present"
    assert "status" in gql_fields, "status field missing"
    assert "message" in gql_fields, "message field missing"
    assert "updated_fields" in gql_fields, "updated_fields field missing"
    assert "id" in gql_fields, "id field missing (entity detected)"

    # Success types should NOT have errors field (v1.9.0+)
    assert "errors" not in gql_fields, "Success types don't have errors field (v1.9.0+)"

    print(f"✅ Python decorator: All Success fields present: {sorted(gql_fields.keys())}")
```

#### Step 5.2: Update Rust integration tests to use correct API

**Problem**: Tests use old `build_graphql_response` API instead of `build_mutation_response`

**Fix test** (lines 49-97):
```python
def test_rust_field_filtering():
    """Verify Rust filters fields based on selection."""
    from fraiseql import _get_fraiseql_rs
    fraiseql_rs = _get_fraiseql_rs()

    # Create test result
    result_dict = {
        "status": "success",
        "message": "Test message",
        "entity_id": "test-123",
        "entity_type": "TestEntity",
        "entity": {"id": "test-123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Test: Only select 'entity' field
    selected_fields = ["entity"]

    response = fraiseql_rs.build_mutation_response(
        json.dumps(result_dict),  # mutation_json (string)
        "testMutation",           # field_name
        "TestSuccess",            # success_type
        "TestError",              # error_type
        "entity",                 # entity_field_name
        "TestEntity",             # entity_type
        None,                     # cascade_selections
        True,                     # auto_camel_case
        selected_fields,          # success_type_fields
    )

    import json
    response_json = json.loads(response)
    data = response_json["data"]["testMutation"]

    # Should have __typename and entity
    assert "__typename" in data
    assert "entity" in data

    # Should NOT have unrequested fields
    assert "id" not in data, f"id should not be present (not requested), got keys: {list(data.keys())}"
    assert "message" not in data, "message should not be present"
    assert "status" not in data, "status should not be present"
    assert "updatedFields" not in data, "updatedFields should not be present"

    # Success types don't have errors field
    assert "errors" not in data, "Success types don't have errors field"

    print(f"✅ Rust filtering: Only requested fields present: {list(data.keys())}")
```

**Fix test** (lines 100-142):
```python
def test_rust_no_selection_returns_all():
    """Verify backward compatibility - no selection returns all fields."""
    from fraiseql import _get_fraiseql_rs
    import json
    fraiseql_rs = _get_fraiseql_rs()

    result_dict = {
        "status": "success",
        "message": "Test message",
        "entity_id": "test-123",
        "entity_type": "TestEntity",
        "entity": {"id": "test-123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # No field selection (None)
    response = fraiseql_rs.build_mutation_response(
        json.dumps(result_dict),
        "testMutation",
        "TestSuccess",
        "TestError",
        "entity",
        "TestEntity",
        None,
        True,
        None,  # No selection - should return all Success-type fields
    )

    response_json = json.loads(response)
    data = response_json["data"]["testMutation"]

    # All Success-type fields should be present
    assert "id" in data, "id should be present (no selection)"
    assert "message" in data, "message should be present"
    assert "status" in data, "status should be present"
    assert "entity" in data, "entity should be present"
    assert "updatedFields" in data, "updatedFields should be present"

    # Success types should NOT have errors field
    assert "errors" not in data, "Success types don't have errors (v1.9.0+)"

    print(f"✅ Backward compat: All Success fields present with None selection: {list(data.keys())}")
```

**Fix test** (lines 145-191):
```python
def test_partial_field_selection():
    """Verify partial field selection works correctly."""
    from fraiseql import _get_fraiseql_rs
    import json
    fraiseql_rs = _get_fraiseql_rs()

    result_dict = {
        "status": "success",
        "message": "Test message",
        "entity_id": "test-123",
        "entity_type": "TestEntity",
        "entity": {"id": "test-123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Select status, message, and entity
    selected_fields = ["status", "message", "entity"]

    response = fraiseql_rs.build_mutation_response(
        json.dumps(result_dict),
        "testMutation",
        "TestSuccess",
        "TestError",
        "entity",
        "TestEntity",
        None,
        True,
        selected_fields,
    )

    response_json = json.loads(response)
    data = response_json["data"]["testMutation"]

    # Requested fields should be present
    assert "status" in data
    assert "message" in data
    assert "entity" in data

    # Unrequested fields should NOT be present
    assert "id" not in data, "id not requested"
    assert "updatedFields" not in data, "updatedFields not requested"

    # Success types don't have errors
    assert "errors" not in data, "Success types don't have errors"

    print(f"✅ Partial selection: {list(data.keys())}")
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Run fixed integration tests
uv run pytest tests/test_mutation_field_selection_integration.py -v

# Expected: 5/5 tests passing
```

### Acceptance Criteria

- [ ] `test_decorator_adds_fields_to_gql_fields` - PASSING
- [ ] `test_failure_decorator_adds_fields` - PASSING (no changes needed)
- [ ] `test_rust_field_filtering` - PASSING
- [ ] `test_rust_no_selection_returns_all` - PASSING
- [ ] `test_partial_field_selection` - PASSING
- [ ] Total: 5/5 tests passing

### Rollback Plan

```bash
git checkout tests/test_mutation_field_selection_integration.py
```

---

## Phase 6: Performance Benchmarks

**Objective**: Add performance tests to measure field filtering overhead

**Duration**: 1 hour

### Files to Create

- `tests/unit/mutations/test_field_selection_performance.py` (new file)

### Implementation Steps

#### Step 6.1: Create performance benchmark file

**Create file**: `tests/unit/mutations/test_field_selection_performance.py`

```python
"""Performance benchmarks for field selection filtering."""
import json
import time
import pytest
from fraiseql import _get_fraiseql_rs


@pytest.fixture
def fraiseql_rs():
    """Get Rust module."""
    return _get_fraiseql_rs()


def test_performance_small_response_field_filtering(fraiseql_rs, benchmark=None):
    """Benchmark field filtering on small response (5 fields, request 2)."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    selected_fields = ["status", "machine"]

    def run_filtering():
        return fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            selected_fields,
        )

    if benchmark:
        # Using pytest-benchmark
        result = benchmark(run_filtering)
    else:
        # Manual timing
        start = time.perf_counter()
        iterations = 10000
        for _ in range(iterations):
            run_filtering()
        end = time.perf_counter()
        avg_time = (end - start) / iterations
        print(f"✅ Small response: {avg_time*1000:.3f}ms avg ({iterations} iterations)")


def test_performance_medium_response_field_filtering(fraiseql_rs, benchmark=None):
    """Benchmark field filtering on medium response (20 fields, request 5)."""

    # Create response with many auto-injected fields + entity
    fake_result = {
        "status": "success",
        "message": "Machine created with full configuration",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {
            "id": "123",
            "name": "Machine X",
            "serial_number": "SN-001",
            "model": "Model A",
            "manufacturer": "Manufacturer B",
            "location": {"id": "loc1", "name": "Warehouse A"},
            "contract": {"id": "c1", "name": "Contract 1"},
            "status": "active",
            "notes": "Some notes here",
            "metadata": {"key1": "value1", "key2": "value2"}
        },
        "updated_fields": ["name", "serial_number", "model", "location_id"],
        "cascade": {
            "deleted": {},
            "updated": {}
        },
        "metadata": None,
        "is_simple_format": False,
    }

    # Request only 5 fields out of many available
    selected_fields = ["status", "message", "machine", "updatedFields", "id"]

    def run_filtering():
        return fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            selected_fields,
        )

    if benchmark:
        result = benchmark(run_filtering)
    else:
        start = time.perf_counter()
        iterations = 5000
        for _ in range(iterations):
            run_filtering()
        end = time.perf_counter()
        avg_time = (end - start) / iterations
        print(f"✅ Medium response: {avg_time*1000:.3f}ms avg ({iterations} iterations)")


def test_performance_large_cascade_field_filtering(fraiseql_rs, benchmark=None):
    """Benchmark field filtering on response with large cascade (100 entities)."""

    # Create 100 reservation entities in cascade
    cascade_entities = [
        {"id": f"r{i}", "name": f"Reservation {i}", "status": "cancelled"}
        for i in range(100)
    ]

    fake_result = {
        "status": "success",
        "message": "Machine deleted with cascade",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Machine X"},
        "updated_fields": [],
        "cascade": {
            "deleted": {"Reservation": cascade_entities},
            "updated": {}
        },
        "metadata": None,
        "is_simple_format": False,
    }

    # Request cascade but filter entity fields
    selected_fields = ["status", "machine", "cascade"]
    cascade_selections = {"Reservation": ["id", "status"]}  # Don't request 'name'

    def run_filtering():
        return fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "deleteMachine",
            "DeleteMachineSuccess",
            "DeleteMachineError",
            "machine",
            "Machine",
            json.dumps(cascade_selections),
            True,
            selected_fields,
        )

    if benchmark:
        result = benchmark(run_filtering)
    else:
        start = time.perf_counter()
        iterations = 1000
        for _ in range(iterations):
            run_filtering()
        end = time.perf_counter()
        avg_time = (end - start) / iterations
        print(f"✅ Large cascade: {avg_time*1000:.3f}ms avg ({iterations} iterations)")


def test_performance_no_filtering_vs_filtering(fraiseql_rs):
    """Compare performance: filtering vs no filtering."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test", "serial": "SN-001", "model": "A"},
        "updated_fields": ["name", "serial"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Test 1: No filtering (None)
    iterations = 5000
    start = time.perf_counter()
    for _ in range(iterations):
        fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            None,  # No filtering
        )
    no_filter_time = time.perf_counter() - start

    # Test 2: With filtering (request 2 fields)
    start = time.perf_counter()
    for _ in range(iterations):
        fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            ["status", "machine"],  # Filtering
        )
    with_filter_time = time.perf_counter() - start

    no_filter_avg = (no_filter_time / iterations) * 1000
    with_filter_avg = (with_filter_time / iterations) * 1000
    overhead = with_filter_avg - no_filter_avg

    print(f"✅ No filtering: {no_filter_avg:.3f}ms avg")
    print(f"✅ With filtering: {with_filter_avg:.3f}ms avg")
    print(f"✅ Overhead: {overhead:.3f}ms ({(overhead/no_filter_avg)*100:.1f}%)")

    # Filtering should have minimal overhead (< 20% slower)
    assert overhead < (no_filter_avg * 0.2), \
        f"Field filtering overhead too high: {overhead:.3f}ms ({(overhead/no_filter_avg)*100:.1f}%)"


def test_performance_canary():
    """Canary: Field filtering performance regression detector."""
    from fraiseql import _get_fraiseql_rs
    fraiseql_rs = _get_fraiseql_rs()

    fake_result = {
        "status": "success",
        "message": "Test",
        "entity_id": "123",
        "entity_type": "Test",
        "entity": {"id": "123"},
        "updated_fields": [],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Single call should be < 1ms
    start = time.perf_counter()
    fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "test",
        "TestSuccess",
        "TestError",
        "entity",
        "Test",
        None,
        True,
        ["status"],
    )
    elapsed = (time.perf_counter() - start) * 1000

    print(f"✅ Single call: {elapsed:.3f}ms")

    # If this fails, field filtering has severe performance regression
    assert elapsed < 5.0, \
        f"Field filtering is too slow: {elapsed:.3f}ms (expected < 5ms)"
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Run performance tests
uv run pytest tests/unit/mutations/test_field_selection_performance.py -v -s

# Optional: Run with pytest-benchmark if installed
uv pip install pytest-benchmark
uv run pytest tests/unit/mutations/test_field_selection_performance.py -v --benchmark-only

# Expected: All tests passing with performance metrics printed
```

### Acceptance Criteria

- [ ] `test_performance_small_response_field_filtering` - PASSING
- [ ] `test_performance_medium_response_field_filtering` - PASSING
- [ ] `test_performance_large_cascade_field_filtering` - PASSING
- [ ] `test_performance_no_filtering_vs_filtering` - PASSING
- [ ] `test_performance_canary` - PASSING
- [ ] Performance overhead < 20%
- [ ] Single call latency < 5ms

### Rollback Plan

```bash
rm tests/unit/mutations/test_field_selection_performance.py
```

---

## Phase 7: Documentation & Cleanup

**Objective**: Document test organization and clean up obsolete code

**Duration**: 0.5 hours

### Files to Create/Modify

- `tests/unit/mutations/README.md` (new)
- Update test docstrings

### Implementation Steps

#### Step 7.1: Create test README

**Create file**: `tests/unit/mutations/README.md`

```markdown
# Mutation Tests

This directory contains unit and integration tests for FraiseQL mutation functionality.

## Test Organization

### Field Selection Tests

Field selection/filtering ensures that auto-injected mutation response fields are only returned when explicitly requested by the client.

**Files**:
- `test_rust_field_selection.py` - Rust layer field filtering (Success/Error types)
- `test_named_fragments.py` - Named fragment support
- `test_field_selection_performance.py` - Performance benchmarks
- `../test_mutation_field_selection_integration.py` - E2E integration tests
- `../../integration/graphql/mutations/test_selection_filter.py` - Python layer selection utilities

### Coverage Matrix

| Scenario | Test File | Status |
|----------|-----------|--------|
| Success type field filtering | `test_rust_field_selection.py` | ✅ Complete |
| Error type field filtering | `test_rust_field_selection.py` | ✅ Complete |
| Named fragments | `test_named_fragments.py` | ✅ Complete |
| Cascade field selection | `test_rust_field_selection.py` | ✅ Complete |
| Multiple entities (v1.8.1) | `test_rust_field_selection.py` | ✅ Complete |
| Performance benchmarks | `test_field_selection_performance.py` | ✅ Complete |
| E2E integration | `test_mutation_field_selection_integration.py` | ✅ Complete |

## Auto-Injected Fields (v1.8.1)

### Success Types
- `status: String!` - Operation status (e.g., "created", "updated")
- `message: String` - Human-readable message
- `id: UUID` - ID of created/updated entity (if entity field present)
- `updatedFields: [String!]` - List of fields that were updated

### Error Types
- `status: String!` - Error status (e.g., "failed:validation")
- `message: String` - Human-readable error message
- `code: Int!` - HTTP-like error code (computed from status)
- `errors: [Error!]` - Detailed error array

**Breaking Changes (v1.8.1)**:
- ❌ Success types do NOT have `errors` field (removed for semantic correctness)
- ❌ Error types do NOT have `id` or `updatedFields` fields (errors = no entity created)

## Field Selection Examples

### GraphQL Query
```graphql
mutation CreateMachine($input: CreateMachineInput!) {
    createMachine(input: $input) {
        # Only request specific fields
        status
        machine { id name }
        # Do NOT request: message, id, updatedFields
    }
}
```

### Response (with field selection)
```json
{
    "data": {
        "createMachine": {
            "__typename": "CreateMachineSuccess",
            "status": "created",
            "machine": { "id": "123", "name": "Machine X" }
            // message, id, updatedFields are NOT included
        }
    }
}
```

### Benefits
- ✅ Reduced bandwidth (only requested fields)
- ✅ GraphQL spec compliance
- ✅ Better performance (less serialization)
- ✅ Cleaner API responses

## Running Tests

```bash
# All mutation tests
uv run pytest tests/unit/mutations/ -v

# Field selection only
uv run pytest tests/unit/mutations/test_rust_field_selection.py -v
uv run pytest tests/unit/mutations/test_named_fragments.py -v

# Performance benchmarks
uv run pytest tests/unit/mutations/test_field_selection_performance.py -v -s

# E2E integration
uv run pytest tests/test_mutation_field_selection_integration.py -v
```

## Debugging Field Selection Issues

If field selection isn't working:

1. **Check FraiseQL version**: Must be v1.8.1+ (commit eaa1f78f or later)
2. **Enable debug logging**:
   ```bash
   export FRAISEQL_DEBUG_FIELD_EXTRACTION=1
   uv run pytest tests/unit/mutations/test_rust_field_selection.py -xvs
   ```
3. **Verify field extraction**: Check that `_extract_selected_fields()` returns correct set
4. **Verify Rust API**: Ensure using `build_mutation_response()` not old `build_graphql_response()`

## Performance Expectations

| Response Size | Filtering Overhead | Single Call Latency |
|---------------|-------------------|---------------------|
| Small (5 fields) | < 10% | < 1ms |
| Medium (20 fields) | < 15% | < 2ms |
| Large (100+ cascade) | < 20% | < 5ms |

If performance degrades beyond these thresholds, check `test_field_selection_performance.py` canary tests.

## Related Documentation

- FraiseQL v1.8.1 CHANGELOG: `/home/lionel/code/fraiseql/CHANGELOG.md`
- Implementation plan: `.phases/fraiseql-auto-injection-redesign/IMPLEMENTATION_PLAN.md`
- Python field extraction: `src/fraiseql/mutations/mutation_decorator.py`
- Rust field filtering: `fraiseql_rs/src/mutations/response_builder.rs`
```

#### Step 7.2: Update test docstrings

**Add docstring improvements to key tests** (examples):

```python
# In test_rust_field_selection.py

def test_rust_filters_success_fields_correctly(fraiseql_rs):
    """
    Verify Rust only returns requested fields in Success response.

    This is the PRIMARY test for field selection. If this fails, field selection is broken.

    Tests:
    - Auto-injected fields (status, message, id, updatedFields) are filtered
    - Only requested fields appear in response
    - __typename is always present (GraphQL requirement)

    Related: test_error_type_filters_auto_injected_fields (for Error types)
    """
    # ... test code ...
```

### Verification Commands

```bash
cd /home/lionel/code/fraiseql

# Verify README is readable
cat tests/unit/mutations/README.md

# Verify all tests still passing
uv run pytest tests/unit/mutations/ -v
uv run pytest tests/test_mutation_field_selection_integration.py -v
```

### Acceptance Criteria

- [ ] `tests/unit/mutations/README.md` created
- [ ] README documents test organization
- [ ] README provides debugging guidance
- [ ] README includes performance expectations
- [ ] Key tests have improved docstrings
- [ ] All tests still passing

### Rollback Plan

```bash
rm tests/unit/mutations/README.md
git checkout tests/unit/mutations/test_rust_field_selection.py  # if docstrings added
```

---

## Migration Strategy for Outdated Tests

### Decision Matrix

| Test File | Current Status | Decision | Rationale |
|-----------|---------------|----------|-----------|
| `test_rust_field_selection.py` | 1/4 passing | **FIX** | Core functionality, just needs updates for v1.8.1+ |
| `test_mutation_field_selection_integration.py` | 4/5 failing | **FIX** | E2E tests valuable, just needs API/field updates |
| `test_selection_filter.py` | All passing | **KEEP** | Already correct, no changes needed |

### Migration Steps Per File

#### `test_rust_field_selection.py`

**Root Cause**: Tests expect `errors` field on Success types (removed in v1.9.0+)

**Migration**:
1. Remove `errors` from Success type assertions
2. Fix Error response test fixture
3. Update to use `build_mutation_response` API

**Effort**: 1.5 hours (Phase 1)

#### `test_mutation_field_selection_integration.py`

**Root Cause**:
- Tests expect `errors` on Success types
- Tests use old Rust API (`build_graphql_response` → `build_mutation_response`)

**Migration**:
1. Remove `errors` from Success type assertions
2. Update Rust API calls to new signature
3. Add `json.dumps()` for mutation_json parameter

**Effort**: 1.5 hours (Phase 5)

---

## Risk Assessment

### Technical Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Rust API signature changed | High | Low | Each phase tests independently |
| Field extraction logic broken | High | Low | Existing tests pass, we're adding coverage |
| Performance regression | Medium | Low | Phase 6 benchmarks detect this |
| Named fragment implementation incomplete | Medium | Medium | Phase 3 tests will reveal issues |

### Schedule Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Tests reveal Rust bugs | +2-4 hours | Each phase independent, can pause and fix |
| Performance tests fail | +1-2 hours | Performance tests are P3, can defer |
| E2E tests complex to fix | +1-2 hours | Can use old tests as reference |

### Backward Compatibility

**Low Risk**:
- Field selection is an **optimization**, not a breaking change
- If `selected_fields = None`, all fields returned (backward compat)
- Tests are **additive** (not changing behavior)

**No Database Changes**:
- Field selection happens in Rust/Python layer
- No PostgreSQL schema changes required

---

## Resource Requirements

### Developer Time

| Role | Effort | Tasks |
|------|--------|-------|
| **Senior Developer** | 8-9 hours | Execute all phases, write tests |
| **Code Reviewer** | 1-2 hours | Review test quality, verify coverage |
| **QA Engineer** | 1 hour | Run tests, verify benchmarks |

**Total**: 10-12 person-hours

### Infrastructure

- ✅ No additional infrastructure needed
- ✅ Tests run on existing CI/CD
- ✅ Optional: `pytest-benchmark` for better performance metrics

### Dependencies

- ✅ FraiseQL v1.8.1+ (already available)
- ✅ Rust module compiled (already available)
- ⚠️ Optional: `pytest-benchmark` (can install in Phase 6)

---

## Success Metrics

After completing all phases:

### Test Coverage

- ✅ **100% passing** in `test_rust_field_selection.py` (13 tests)
- ✅ **100% passing** in `test_named_fragments.py` (6 tests)
- ✅ **100% passing** in `test_field_selection_performance.py` (5 tests)
- ✅ **100% passing** in `test_mutation_field_selection_integration.py` (5 tests)
- ✅ **Total**: 29+ tests passing

### Coverage Areas

| Area | Before | After | Improvement |
|------|--------|-------|-------------|
| Success field filtering | Partial | Complete | +7 tests |
| Error field filtering | Broken | Complete | +4 tests |
| Named fragments | Missing | Complete | +6 tests |
| Edge cases | Missing | Complete | +5 tests |
| E2E integration | Broken | Complete | Fixed |
| Performance | Missing | Complete | +5 tests |

### Performance

- ✅ Field filtering overhead < 20%
- ✅ Single call latency < 5ms
- ✅ Canary tests prevent regressions

### Documentation

- ✅ Test README explains organization
- ✅ Debugging guidance available
- ✅ Performance expectations documented
- ✅ Test docstrings improved

---

## Timeline & Execution Order

### Week 1, Day 1 (Morning: 4 hours)

- **09:00-09:30**: Phase 0 - Assessment
- **09:30-11:00**: Phase 1 - Fix Outdated Tests
- **11:00-12:30**: Phase 2 - Error Type Tests
- **12:30-13:00**: Lunch

### Week 1, Day 1 (Afternoon: 4 hours)

- **13:00-14:00**: Phase 3 - Named Fragment Tests
- **14:00-15:30**: Phase 4 - Edge Case Tests
- **15:30-17:00**: Phase 5 - E2E Integration Tests

### Week 1, Day 2 (Morning: 2 hours)

- **09:00-10:00**: Phase 6 - Performance Benchmarks
- **10:00-10:30**: Phase 7 - Documentation
- **10:30-11:00**: Final verification and cleanup

**Total**: 10 hours (with buffer for debugging)

---

## Rollback Instructions

### Per-Phase Rollback

Each phase creates a patch file for easy rollback:

```bash
# Phase 1
git checkout tests/unit/mutations/test_rust_field_selection.py

# Phase 2
git apply --reverse /tmp/phase2.patch

# Phase 3
rm tests/unit/mutations/test_named_fragments.py

# Phase 4
git apply --reverse /tmp/phase4.patch

# Phase 5
git checkout tests/test_mutation_field_selection_integration.py

# Phase 6
rm tests/unit/mutations/test_field_selection_performance.py

# Phase 7
rm tests/unit/mutations/README.md
```

### Complete Rollback

```bash
cd /home/lionel/code/fraiseql
git status  # Check what changed
git diff tests/  # Review changes
git checkout tests/  # Restore all test files
git clean -fd tests/  # Remove new files
```

---

## Code Examples Appendix

### Example 1: Success Type Field Filtering

**GraphQL Query**:
```graphql
mutation CreateMachine($input: CreateMachineInput!) {
    createMachine(input: $input) {
        status
        machine { id name }
    }
}
```

**Response** (with field selection):
```json
{
    "data": {
        "createMachine": {
            "__typename": "CreateMachineSuccess",
            "status": "created",
            "machine": { "id": "123", "name": "Machine X" }
        }
    }
}
```

**Without field selection** (all auto-injected fields):
```json
{
    "data": {
        "createMachine": {
            "__typename": "CreateMachineSuccess",
            "status": "created",
            "message": "Machine created successfully",
            "id": "123",
            "updatedFields": ["name", "serial_number"],
            "machine": { "id": "123", "name": "Machine X" }
        }
    }
}
```

### Example 2: Error Type Field Filtering

**GraphQL Query**:
```graphql
mutation CreateMachine($input: CreateMachineInput!) {
    createMachine(input: $input) {
        ... on CreateMachineError {
            code
            errors { code message }
        }
    }
}
```

**Response**:
```json
{
    "data": {
        "createMachine": {
            "__typename": "CreateMachineError",
            "code": 422,
            "errors": [
                { "code": "VALIDATION_ERROR", "message": "Serial number required" }
            ]
        }
    }
}
```

### Example 3: Named Fragment

**GraphQL Query**:
```graphql
fragment MachineFields on CreateMachineSuccess {
    status
    message
    machine { id name serialNumber }
}

mutation CreateMachine($input: CreateMachineInput!) {
    createMachine(input: $input) {
        ...MachineFields
    }
}
```

**Field Extraction**:
```python
# _extract_selected_fields(info) returns:
{"status", "message", "machine"}
```

---

## Questions & Answers

### Q1: Should we fix or remove `test_mutation_field_selection_integration.py`?

**A**: **FIX** - The E2E integration tests are valuable for verifying the full Python → Rust pipeline. The tests just need updates for v1.8.1+ behavior (remove `errors` from Success, update Rust API).

### Q2: Where should field selection tests live?

**A**: Current organization is good:
- **Unit tests**: `tests/unit/mutations/` - Rust layer, named fragments, performance
- **Integration tests**: `tests/integration/graphql/mutations/` - Python layer utilities
- **E2E tests**: `tests/` (root level) - Full pipeline tests

### Q3: Test naming convention?

**A**: Use descriptive names that explain **what** is being tested:
- ✅ `test_rust_filters_success_fields_correctly`
- ✅ `test_error_type_filters_auto_injected_fields`
- ✅ `test_named_fragment_field_extraction`
- ❌ `test_field_selection_works` (too vague)

### Q4: How to test Error types don't have Success fields?

**A**: Use negative assertions:
```python
# Request fields that shouldn't exist
selected_fields = ["code", "status", "id", "updatedFields"]

response = build_mutation_response(...)

# Error types should silently ignore Success-only fields
assert "id" not in response
assert "updatedFields" not in response
```

### Q5: Performance baseline?

**A**: Based on Rust performance:
- Small response (5 fields): < 1ms per call
- Medium response (20 fields): < 2ms per call
- Large cascade (100 entities): < 5ms per call
- Overhead: < 20% vs no filtering

### Q6: Canary tests?

**A**: Added in Phase 6:
```python
def test_performance_canary():
    """If this breaks, field selection has severe regression."""
    # Single call should be < 5ms
    elapsed = measure_single_call()
    assert elapsed < 5.0, "Field filtering too slow"
```

### Q7: Documentation?

**A**: Added in Phase 7:
- `tests/unit/mutations/README.md` - Test organization, examples, debugging
- Improved docstrings in key tests
- Performance expectations documented

---

## Conclusion

This implementation plan provides a **comprehensive, phased approach** to improving field selection test coverage in FraiseQL. Each phase is:

- **Independent**: Can pause/resume at phase boundaries
- **Testable**: Clear verification commands for each phase
- **Rollbackable**: Easy rollback if issues found
- **Incremental**: Builds on previous phases

**Key Outcomes**:
- ✅ All field selection tests passing (100%)
- ✅ Comprehensive coverage (Success, Error, fragments, edge cases)
- ✅ Performance benchmarks prevent regressions
- ✅ E2E integration tests verify full pipeline
- ✅ Clear documentation for future maintenance

**Estimated Effort**: 8-10 hours

**Risk Level**: Low-Medium

**Priority**: High (critical for API correctness)

---

**Ready for execution!** 🚀
