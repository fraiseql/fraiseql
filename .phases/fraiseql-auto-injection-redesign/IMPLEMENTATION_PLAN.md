# FraiseQL Auto-Injection Redesign: Complete Implementation Plan

**Date**: 2025-12-11
**Status**: Ready for Execution
**Estimated Total Effort**: 14-18 hours
**Target Version**: FraiseQL v1.8.1

---

## 🎯 Executive Summary

This document provides a comprehensive, phased implementation plan to clean up FraiseQL's mutation response auto-injection architecture. The goal is to establish a semantically correct, developer-friendly, and GraphQL-compliant system for auto-injecting mutation response fields.

### Key Objectives

1. **Remove Boilerplate**: Auto-inject `code` field on Error types (remove manual requirement)
2. **Semantic Correctness**: Remove semantically incorrect fields (`updated_fields`, `id` on Error types)
3. **Fix Field Extraction**: Support named fragments (not just inline fragments)
4. **Clean Up Dead Code**: Remove temporary workarounds and commented code

### Scope Summary

| Layer | Changes | Files Modified | Effort |
|-------|---------|----------------|--------|
| **Field Extraction** | Fix named fragment support, add diagnostic logging | 1 file | 2 hours |
| **Python Decorators** | Auto-inject `code`, remove `updated_fields`/`id` from errors | 2 files | 2 hours |
| **Rust Response Builder** | Clean up Success response handling | 1 file | 1 hour |
| **PrintOptim Migration** | Update ~45 Error types, ~138 tests (AST-based) | Many files | 4 hours |
| **Testing & Validation** | Unit tests, integration tests, canary tests | 3-4 files | 2 hours |
| **Documentation** | API docs, changelog, examples | 2-3 files | 1-2 hours |

---

## 📊 Current State Analysis

### Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: PostgreSQL Database                               │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│ CREATE TYPE app.mutation_response AS (                     │
│     status          TEXT,                                   │
│     message         TEXT,                                   │
│     entity_id       TEXT,                                   │
│     entity_type     TEXT,                                   │
│     entity          JSONB,                                  │
│     updated_fields  TEXT[],                                 │
│     cascade         JSONB,                                  │
│     metadata        JSONB                                   │
│ );                                                          │
│ ❌ NO errors field                                          │
│ ❌ NO code field                                            │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: Python Decorators (FraiseQL)                      │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│ @fraiseql.success  →  Auto-inject:                         │
│   ✅ status, message, updated_fields, id (conditional)     │
│   ✅ errors ALREADY REMOVED (v1.9.0)                        │
│                                                             │
│ @fraiseql.error    →  Auto-inject:                         │
│   ✅ status, message, errors                                │
│   ⚠️ updated_fields, id (SEMANTICALLY INCORRECT)           │
│   ❌ code (REQUIRED but NOT auto-injected - BOILERPLATE)   │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Rust Response Builder                             │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│ Success Response Builder:                                  │
│   ✅ Field selection filtering                             │
│   ⚠️ Still has code for errors field (lines 154-159)       │
│   ✅ Adds id, status, message, updatedFields if selected   │
│                                                             │
│ Error Response Builder:                                    │
│   ✅ Computes code from status (422, 404, 409, 500)        │
│   ✅ Generates errors array from status                    │
│   ✅ Field selection filtering                             │
└─────────────────────────────────────────────────────────────┘
```

### Current Problems

#### Problem 1: Manual `code` Field Requirement

**Current State**:
```python
@fraiseql.error
class CreateMachineError:
    code: int  # ❌ User must manually add to EVERY Error type
```

**Validation** (mutation_decorator.py:274-286):
```python
if "code" not in error_annotations:
    raise ValueError(
        f"Error type {error_type_name} must have 'code: int' field. "
        f"v1.8.0 requires Error types to include REST-like error codes."
    )
```

**Issues**:
- ❌ Boilerplate: Must add to every Error type
- ❌ Ignored value: Rust computes actual value from `status`
- ❌ Confusing: "Why define if it's ignored?"
- ❌ Validation overhead: Runtime check for manual field

#### Problem 2: Semantically Incorrect Fields on Error Types

**Current State**:
```python
@fraiseql.error
class CreateMachineError:
    # Auto-injected:
    updated_fields: list[str] | None = None  # ❌ Nothing was updated (operation failed)
    id: str | None = None                    # ❌ Nothing was created (operation failed)
```

**Issues**:
- ❌ Semantic confusion: Errors don't update fields or create entities
- ❌ Usually `None`: Wasteful to include
- ❌ Misleading DX: Suggests partial updates are tracked

#### Problem 3: Rust Code for `errors` Field on Success

**Current State** (response_builder.rs:154-159):
```rust
// Add errors array if selected (empty for success responses)
if is_selected("errors") {
    eprintln!("    Adding 'errors' to response");
    obj.insert("errors".to_string(), json!([]));
}
```

**Issues**:
- ⚠️ Dead code: Python already removed `errors` from Success types (decorators.py:111-113)
- ⚠️ Backward compatibility artifact: Can be removed
- ⚠️ Confusing: Code suggests Success can have errors

#### Problem 4: Field Extraction Reliability

**Current State** (mutation_decorator.py:19-66):
- Field extraction sometimes returns `None`
- Causes Rust to use backward-compatibility mode (returns ALL fields)
- Missing fields in response indicate database doesn't include them

**Issues**:
- ❌ Unrequested fields appear in response
- ❌ Field filtering not working as intended
- ❌ Diagnostic logging needed for debugging

---

## 🏗️ Proposed Final State (Greenfield)

### Success Types

**Python Definition**:
```python
@fraiseql.success
class CreateMachineSuccess:
    """User defines only entity-specific fields."""
    machine: Machine
    cascade: Cascade | None = None

    # Auto-injected by decorator:
    # ✅ status: str = "success"
    # ✅ message: str | None = None
    # ✅ updated_fields: list[str] | None = None
    # ✅ id: str | None = None  (conditional on has_entity_field)
    # ✅ errors: ALREADY REMOVED in v1.9.0
```

**GraphQL Schema**:
```graphql
type CreateMachineSuccess {
  __typename: String!
  status: String!              # ✅ Auto-injected
  message: String              # ✅ Auto-injected
  updatedFields: [String!]     # ✅ Auto-injected
  id: UUID                     # ✅ Auto-injected (conditional)
  machine: Machine!            # User-defined
  cascade: Cascade             # User-defined
  # NO errors field (removed in v1.9.0)
}
```

### Error Types

**Python Definition**:
```python
@fraiseql.error
class CreateMachineError:
    """User defines nothing - all auto-injected."""
    pass

    # Auto-injected by decorator:
    # ✅ status: str = "error"
    # ✅ message: str
    # ✅ code: int  ← NEW: Auto-injected (remove manual requirement)
    # ✅ errors: list[Error] | None = []
    # ❌ updated_fields: REMOVED
    # ❌ id: REMOVED
```

**GraphQL Schema**:
```graphql
type CreateMachineError {
  __typename: String!
  status: String!              # ✅ Auto-injected
  message: String!             # ✅ Auto-injected
  code: Int!                   # ✅ Auto-injected (NEW - computed from status)
  errors: [Error!]             # ✅ Auto-injected (generated from status)
  # NO updatedFields (nothing was updated)
  # NO id (nothing was created)
}
```

### Field Comparison Matrix

| Field | Database | Success Type | Error Type | Computed/Stored | Rationale |
|-------|----------|--------------|------------|-----------------|-----------|
| **status** | ✅ TEXT | ✅ Auto-inject | ✅ Auto-inject | Stored | Source of truth for operation result |
| **message** | ✅ TEXT | ✅ Auto-inject | ✅ Auto-inject | Stored | Human-readable description |
| **errors** | ❌ NO | ❌ Removed v1.9.0 | ✅ Auto-inject | Computed | Field-level errors (errors only) |
| **code** | ❌ NO | ❌ NO | ✅ **NEW** Auto-inject | Computed | HTTP-like code (422, 404, 409, 500) |
| **updated_fields** | ✅ TEXT[] | ✅ Auto-inject | ❌ **REMOVE** | Stored | Only on success (audit trail) |
| **id** | ✅ TEXT | ✅ Auto-inject (cond.) | ❌ **REMOVE** | Stored | Only on success (convenience) |

---

## 📋 Phased Implementation Plan

### Phase 0: Field Extraction Fix & Diagnostic Tooling

**Objective**: Fix named fragment support and add diagnostic logging

**Duration**: 2 hours

**Tasks**:

1. **Fix named fragment support** (1 hour)
   - File: `src/fraiseql/mutations/mutation_decorator.py`
   - Current: Only handles inline fragments (`... on Type`)
   - Add: Support for named fragments (`...FragmentName`)
   - Extract helper function `_extract_fields_from_selection_set()`
   - Access `info.fragments` to resolve named fragment spreads

2. **Add diagnostic logging to field extraction** (15 min)
   - Same file: `src/fraiseql/mutations/mutation_decorator.py`
   - Add conditional logging (env var: `FRAISEQL_DEBUG_FIELD_EXTRACTION`)
   - Log: type_name, fragment types, extracted fields, named vs inline

3. **Add diagnostic logging to Rust response builder** (15 min)
   - File: `fraiseql_rs/src/mutation/response_builder.rs`
   - Wrap existing diagnostic logging in `#[cfg(debug_assertions)]`
   - Add database response logging

4. **Create test suite for field extraction** (30 min)
   - File: `tests/mutations/test_field_extraction_edge_cases.py`
   - Test: Inline fragments (existing behavior)
   - Test: Named fragments (NEW - must work)
   - Test: Mixed inline + named fragments
   - Test: Fragment type mismatch (returns None)

**Implementation Details**:

```python
# src/fraiseql/mutations/mutation_decorator.py

import os

DEBUG_FIELD_EXTRACTION = os.getenv("FRAISEQL_DEBUG_FIELD_EXTRACTION", "0") == "1"

def _extract_fields_from_selection_set(selection_set, field_set: set[str]) -> None:
    """Helper to extract field names from selection set recursively."""
    for field_selection in selection_set.selections:
        if hasattr(field_selection, "name"):
            field_name = field_selection.name.value
            if field_name != "__typename":
                field_set.add(field_name)

def _extract_mutation_selected_fields(info: GraphQLResolveInfo, type_name: str) -> list[str] | None:
    """Extract fields selected on a mutation response type from GraphQL query.

    Supports both inline fragments (... on Type) and named fragments (...FragmentName).
    """
    if DEBUG_FIELD_EXTRACTION:
        logger.warning(f"🔍 FIELD EXTRACTION: type={type_name}")

    if not info or not info.field_nodes:
        return None

    selected_fields = set()

    for field_node in info.field_nodes:
        if not field_node.selection_set:
            continue

        for selection in field_node.selection_set.selections:
            # Inline fragment: ... on Type { fields }
            if hasattr(selection, "type_condition") and selection.type_condition:
                fragment_type = selection.type_condition.name.value

                if DEBUG_FIELD_EXTRACTION:
                    logger.warning(f"  Inline fragment: {fragment_type}, match={fragment_type == type_name}")

                if fragment_type == type_name and selection.selection_set:
                    _extract_fields_from_selection_set(selection.selection_set, selected_fields)

            # Named fragment spread: ...FragmentName
            elif hasattr(selection, "name") and hasattr(info, "fragments"):
                fragment_name = selection.name.value
                fragment = info.fragments.get(fragment_name)

                if fragment and hasattr(fragment, "type_condition"):
                    fragment_type = fragment.type_condition.name.value

                    if DEBUG_FIELD_EXTRACTION:
                        logger.warning(f"  Named fragment: {fragment_name} → {fragment_type}, match={fragment_type == type_name}")

                    if fragment_type == type_name:
                        _extract_fields_from_selection_set(fragment.selection_set, selected_fields)

    if not selected_fields:
        if DEBUG_FIELD_EXTRACTION:
            logger.warning(f"  No fields extracted → backward compat mode")
        return None

    result = list(selected_fields)
    if DEBUG_FIELD_EXTRACTION:
        logger.warning(f"  Extracted: {result}")
    return result
```

**Verification**:
```bash
# 1. Test named fragment support
pytest tests/mutations/test_field_extraction_edge_cases.py::test_named_fragments -xvs

# 2. Run all field extraction tests
pytest tests/mutations/test_field_extraction_edge_cases.py -xvs

# 3. Test with debug logging
FRAISEQL_DEBUG_FIELD_EXTRACTION=1 pytest tests/mutations/ -k field_extraction -xvs
```

**Deliverables**:
- ✅ Named fragment support (fixes field extraction reliability)
- ✅ Conditional diagnostic logging (production-safe)
- ✅ Test suite documenting edge cases
- ✅ Cleaner Rust diagnostic logging

**Acceptance Criteria**:
- ✅ Named fragments work (extract fields correctly)
- ✅ Inline fragments still work (no regression)
- ✅ Mixed fragments work
- ✅ Diagnostic logging helps debug issues
- ✅ All tests pass

**Rollback Plan**:
- Changes are backward compatible (only add named fragment support)
- If issues found, wrap new code in try/except and fall back to current behavior

---

### Phase 1: Python Decorator Changes

**Objective**: Auto-inject `code` on Error types, remove semantically incorrect fields

**Duration**: 3 hours

**Files to Modify**:

#### File 1: `src/fraiseql/mutations/decorators.py`

**Change 1.1: Auto-inject `code` on Error types** (lines 199-204)

**Before**:
```python
if "errors" not in annotations:
    annotations["errors"] = list[Error] | None
    cls.errors = []
    auto_injected_fields.append("errors")

# Add updatedFields  ← Line 206
```

**After**:
```python
if "errors" not in annotations:
    annotations["errors"] = list[Error] | None
    cls.errors = []
    auto_injected_fields.append("errors")

# NEW: Auto-inject code field on Error types
if "code" not in annotations:
    annotations["code"] = int
    cls.code = 0  # Placeholder - Rust computes actual value
    auto_injected_fields.append("code")

# Add updatedFields  ← Line 211 (moved down 5 lines)
```

**Change 1.2: Remove `updated_fields` from Error types** (lines 206-210)

**Before**:
```python
# Add updatedFields
if "updated_fields" not in annotations:
    annotations["updated_fields"] = list[str] | None
    cls.updated_fields = None
    auto_injected_fields.append("updated_fields")

cls.__annotations__ = annotations  ← Line 212
```

**After**:
```python
# updated_fields REMOVED from Error types (semantically incorrect)
# Errors don't update fields - operation failed

cls.__annotations__ = annotations  ← Line 213 (moved down 1 line due to code injection)
```

**Change 1.3: Remove `id` from Error types** (lines 220-223)

**Before**:
```python
# Detect if class has an entity field
has_entity_field = any(
    field_name not in {"status", "message", "errors", "updated_fields", "id"}
    for field_name in annotations
)

if has_entity_field and "id" not in annotations:
    annotations["id"] = str | None
    cls.id = None
    auto_injected_fields.append("id")
```

**After**:
```python
# id field REMOVED from Error types (semantically incorrect)
# Errors don't create/update entities - operation failed
# has_entity_field check removed for Error types
```

**Change 1.4: Update field descriptions** (lines 314-323)

**Before**:
```python
def _get_auto_field_description_failure(field_name: str) -> str:
    """Get description for auto-injected failure fields."""
    descriptions = {
        "status": "Error status code (e.g., 'error', 'failed', 'blocked')",
        "message": "Human-readable error message",
        "errors": "List of detailed error information",
        "updated_fields": "List of field names that would have been updated",  # ← REMOVE
        "id": "ID of the entity that would have been created or updated",      # ← REMOVE
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

**After**:
```python
def _get_auto_field_description_failure(field_name: str) -> str:
    """Get description for auto-injected failure fields."""
    descriptions = {
        "status": "Error status code (e.g., 'error', 'failed', 'blocked')",
        "message": "Human-readable error message",
        "code": "HTTP-like error code (422=validation, 404=not_found, 409=conflict, 500=server_error)",  # ← NEW
        "errors": "List of detailed error information",
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

#### File 2: `src/fraiseql/mutations/mutation_decorator.py`

**Change 1.5: Remove manual `code` validation** (lines 274-286)

**Before**:
```python
error_annotations = self.error_type.__annotations__

# Error must have code field (v1.8.0)
if "code" not in error_annotations:
    raise ValueError(
        f"Error type {error_type_name} must have 'code: int' field. "
        f"v1.8.0 requires Error types to include REST-like error codes."
    )

# Code must be int
code_type = error_annotations["code"]
if code_type != int:  # noqa: E721
    raise ValueError(
        f"Error type {error_type_name} has wrong 'code' type: {code_type}. Expected 'int'."
    )

# Error must have status field  ← Line 288
```

**After**:
```python
error_annotations = self.error_type.__annotations__

# code field validation REMOVED - now auto-injected by @fraiseql.error decorator
# v2.0.0: code is automatically added to all Error types

# Error must have status field  ← Line 273 (moved up 15 lines)
```

**Verification Commands**:
```bash
# 1. Run FraiseQL tests
cd /home/lionel/code/fraiseql
pytest tests/mutations/test_decorators.py -xvs
pytest tests/mutations/test_auto_populate_schema.py -xvs

# 2. Check that code is auto-injected
python -c "
from fraiseql.mutations.decorators import failure

@failure
class TestError:
    pass

print('Annotations:', TestError.__annotations__)
print('code field present:', 'code' in TestError.__annotations__)
print('code type:', TestError.__annotations__.get('code'))
"

# Expected output:
# Annotations: {'status': <class 'str'>, 'message': <class 'str'> | None, 'errors': list[Error] | None, 'code': <class 'int'>}
# code field present: True
# code type: <class 'int'>

# 3. Verify updated_fields and id NOT in Error annotations
python -c "
from fraiseql.mutations.decorators import failure

@failure
class TestError:
    pass

print('updated_fields present:', 'updated_fields' in TestError.__annotations__)
print('id present:', 'id' in TestError.__annotations__)
"

# Expected output:
# updated_fields present: False
# id present: False
```

**Acceptance Criteria**:
- ✅ `code` field auto-injected on all Error types
- ✅ `updated_fields` NOT injected on Error types
- ✅ `id` NOT injected on Error types
- ✅ All FraiseQL core tests pass
- ✅ Manual `code` validation removed
- ✅ Field descriptions updated

**Rollback Plan**:
```bash
# If issues found, revert changes
cd /home/lionel/code/fraiseql
git checkout HEAD -- src/fraiseql/mutations/decorators.py
git checkout HEAD -- src/fraiseql/mutations/mutation_decorator.py
pytest tests/mutations/ -xvs  # Verify rollback works
```

**DO NOT**:
- ❌ Remove `status`, `message`, or `errors` from Error types
- ❌ Change Success type auto-injection (already correct)
- ❌ Modify database layer (out of scope)
- ❌ Change Rust code yet (Phase 2)

---

### Phase 2: Rust Response Builder Changes

**Objective**: Clean up Success response handling, ensure Error response works with auto-injected `code`

**Duration**: 3 hours

**Files to Modify**:

#### File 1: `fraiseql_rs/src/mutation/response_builder.rs`

**Change 2.1: Remove `errors` field from Success response** (lines 153-159)

**Before**:
```rust
// Add errors array if selected (empty for success responses)
if is_selected("errors") {
    eprintln!("    Adding 'errors' to response");
    obj.insert("errors".to_string(), json!([]));
} else {
    eprintln!("    SKIPPING 'errors' (not selected)");
}

// v1.8.0: SUCCESS MUST HAVE ENTITY (non-null guarantee)  ← Line 162
```

**After**:
```rust
// errors field removed from Success responses in v1.9.0
// Success types don't have errors - that's semantically incorrect
// errors field still exists on Error types where it belongs

// v1.8.0: SUCCESS MUST HAVE ENTITY (non-null guarantee)  ← Line 156 (moved up 6 lines)
```

**Change 2.2: Remove `updatedFields` and `id` from Error response**

**Analysis**: Error response builder (`build_error_response_with_code`, lines 317-365) does NOT currently add `updatedFields` or `id` fields. No changes needed.

**Verification**: Check that error response only includes:
- `__typename`
- `code` (if selected)
- `status` (if selected)
- `message` (if selected)
- `errors` (if selected)
- `cascade` (if selected)

**Change 2.3: Update diagnostic logging** (lines 98-125)

**Before**:
```rust
eprintln!("🔍 RUST FIELD SELECTION DEBUG:");
eprintln!("  Type: {}", success_type);
eprintln!("  success_type_fields: {:?}", success_type_fields);
eprintln!("  should_filter: {}", success_type_fields.is_some());
```

**After**:
```rust
// Optional: Reduce verbosity or make conditional on debug flag
#[cfg(debug_assertions)]
{
    eprintln!("🔍 RUST FIELD SELECTION DEBUG:");
    eprintln!("  Type: {}", success_type);
    eprintln!("  success_type_fields: {:?}", success_type_fields);
    eprintln!("  should_filter: {}", success_type_fields.is_some());
}
```

**Verification Commands**:
```bash
# 1. Rebuild Rust extension
cd /home/lionel/code/fraiseql
maturin develop --release

# 2. Run Rust tests
cargo test --package fraiseql_rs --lib mutation::response_builder::tests

# 3. Run integration tests
pytest tests/mutations/test_rust_field_selection.py -xvs

# 4. Verify Success response does NOT include errors field
python -c "
from fraiseql_rs import build_success_response
import json

result = {
    'status': 'success',
    'message': 'Created',
    'entity_id': '123',
    'entity_type': 'Machine',
    'entity': {'id': '123', 'name': 'Test'},
    'updated_fields': ['name'],
    'cascade': None,
    'metadata': None,
}

response = build_success_response(
    result,
    'CreateMachineSuccess',
    'machine',
    True,
    ['status', 'message', 'machine'],  # NOT requesting errors
    None
)

obj = json.loads(response)
print('errors field present:', 'errors' in obj)
print('Fields present:', list(obj.keys()))
"

# Expected output:
# errors field present: False
# Fields present: ['__typename', 'status', 'message', 'machine']

# 5. Verify Error response includes auto-injected code
python -c "
from fraiseql_rs import build_error_response_with_code
import json

result = {
    'status': 'noop:invalid_id',
    'message': 'Invalid ID',
    'entity_id': None,
    'entity_type': None,
    'entity': None,
    'updated_fields': None,
    'cascade': None,
    'metadata': None,
}

response = build_error_response_with_code(
    result,
    'CreateMachineError',
    True,
    ['code', 'status', 'message', 'errors'],
    None
)

obj = json.loads(response)
print('code field present:', 'code' in obj)
print('code value:', obj.get('code'))
print('Fields present:', list(obj.keys()))
print('updatedFields present:', 'updatedFields' in obj)
print('id present:', 'id' in obj)
"

# Expected output:
# code field present: True
# code value: 422
# Fields present: ['__typename', 'code', 'status', 'message', 'errors']
# updatedFields present: False
# id present: False
```

**Acceptance Criteria**:
- ✅ Success responses do NOT include `errors` field
- ✅ Error responses do NOT include `updatedFields` or `id` fields
- ✅ Error responses include `code` field (computed from status)
- ✅ All Rust tests pass
- ✅ Integration tests pass
- ✅ Diagnostic logging updated/reduced

**Rollback Plan**:
```bash
# If issues found, revert Rust changes
cd /home/lionel/code/fraiseql
git checkout HEAD -- fraiseql_rs/src/mutation/response_builder.rs
maturin develop --release
pytest tests/mutations/ -xvs
```

**DO NOT**:
- ❌ Change status-to-code mapping (lines 426-458)
- ❌ Modify error generation logic (lines 367-389)
- ❌ Change CASCADE handling
- ❌ Modify field selection filtering logic

---

### Phase 3: Testing & Validation

**Objective**: Ensure all FraiseQL tests pass with new auto-injection behavior

**Duration**: 2 hours

**Tasks**:

#### Task 3.1: Update FraiseQL Core Tests (1 hour)

**Files to check/update**:

1. **`tests/mutations/test_decorators.py`**
   - Update tests that check Error type annotations
   - Verify `code` is auto-injected
   - Verify `updated_fields` and `id` are NOT injected on Error types

2. **`tests/mutations/test_auto_populate_schema.py`**
   - Update schema validation tests
   - Verify Error types have `code` field in GraphQL schema
   - Verify Error types do NOT have `updatedFields` or `id` in schema

3. **`tests/mutations/test_mutation_decorator.py`**
   - Remove/update tests that validate manual `code` field
   - Verify validation no longer requires manual `code` field

4. **`tests/mutations/test_rust_field_selection.py`**
   - Verify Success responses don't include `errors` field
   - Verify Error responses include `code` field
   - Verify Error responses don't include `updatedFields` or `id`

**Example Test Updates**:

**Before**:
```python
def test_error_type_requires_code_field():
    """Error types must have manual code field."""
    with pytest.raises(ValueError, match="must have 'code: int' field"):
        @fraiseql.error
        class TestError:
            pass  # Missing code field
```

**After**:
```python
def test_error_type_auto_injects_code_field():
    """Error types automatically get code field."""
    @fraiseql.error
    class TestError:
        pass

    assert "code" in TestError.__annotations__
    assert TestError.__annotations__["code"] == int
```

#### Task 3.2: Run Full Test Suite (30 min)

```bash
cd /home/lionel/code/fraiseql

# 1. Run Python tests
pytest tests/mutations/ -xvs --tb=short

# 2. Run Rust tests
cargo test --package fraiseql_rs

# 3. Run integration tests
pytest tests/integration/ -xvs

# 4. Check coverage (optional)
pytest tests/mutations/ --cov=fraiseql.mutations --cov-report=term-missing
```

#### Task 3.3: Add Canary Tests (30 min)

**Create canary tests** (`tests/mutations/test_canary.py`):
```python
"""Canary tests - will break if field auto-injection changes unexpectedly."""

import pytest
from fraiseql.mutations.decorators import success, failure


def test_success_type_fields_canary():
    """Canary: Success type fields should not change unexpectedly."""

    @success
    class TestSuccess:
        entity: dict

    # Expected auto-injected fields (v1.8.1)
    expected = {"status", "message", "updated_fields", "id", "entity"}
    actual = set(TestSuccess.__annotations__.keys())

    assert actual == expected, (
        f"❌ Success type fields changed!\n"
        f"   Expected: {expected}\n"
        f"   Got:      {actual}\n"
        f"   Missing:  {expected - actual}\n"
        f"   Extra:    {actual - expected}"
    )


def test_error_type_fields_canary():
    """Canary: Error type fields should not change unexpectedly."""

    @failure
    class TestError:
        pass

    # Expected auto-injected fields (v1.8.1)
    expected = {"status", "message", "code", "errors"}
    actual = set(TestError.__annotations__.keys())

    assert actual == expected, (
        f"❌ Error type fields changed!\n"
        f"   Expected: {expected}\n"
        f"   Got:      {actual}\n"
        f"   Missing:  {expected - actual}\n"
        f"   Extra:    {actual - expected}"
    )


def test_error_type_no_update_fields_canary():
    """Canary: Error types should NOT have updatedFields or id."""

    @failure
    class TestError:
        pass

    forbidden = {"updated_fields", "id"}
    actual = set(TestError.__annotations__.keys())

    unexpected = forbidden & actual
    assert not unexpected, (
        f"❌ Error type has forbidden fields: {unexpected}\n"
        f"   Error types should NOT have: {forbidden}\n"
        f"   Actual fields: {actual}"
    )


def test_success_type_no_error_fields_canary():
    """Canary: Success types should NOT have code or errors."""

    @success
    class TestSuccess:
        entity: dict

    forbidden = {"code", "errors"}
    actual = set(TestSuccess.__annotations__.keys())

    unexpected = forbidden & actual
    assert not unexpected, (
        f"❌ Success type has forbidden fields: {unexpected}\n"
        f"   Success types should NOT have: {forbidden}\n"
        f"   Actual fields: {actual}"
    )
```

**Run canary tests**:
```bash
cd /home/lionel/code/fraiseql
pytest tests/mutations/test_canary.py -xvs

# Expected: All pass
```

#### Task 3.4: Manual Testing (30 min)

**Create test script** (`manual_test_auto_injection.py`):
```python
"""Manual test to verify auto-injection behavior."""

from fraiseql.mutations.decorators import success, failure
from fraiseql.types.errors import Error


@success
class TestSuccess:
    """Test success type."""
    entity: dict


@failure
class TestError:
    """Test error type."""
    pass


def test_success_annotations():
    """Verify Success type annotations."""
    print("\n=== Success Type Annotations ===")
    print(f"Annotations: {TestSuccess.__annotations__}")

    # Should have these fields
    assert "status" in TestSuccess.__annotations__
    assert "message" in TestSuccess.__annotations__
    assert "updated_fields" in TestSuccess.__annotations__
    assert "id" in TestSuccess.__annotations__

    # Should NOT have these fields
    assert "errors" not in TestSuccess.__annotations__
    assert "code" not in TestSuccess.__annotations__

    print("✅ Success type annotations correct")


def test_error_annotations():
    """Verify Error type annotations."""
    print("\n=== Error Type Annotations ===")
    print(f"Annotations: {TestError.__annotations__}")

    # Should have these fields
    assert "status" in TestError.__annotations__
    assert "message" in TestError.__annotations__
    assert "code" in TestError.__annotations__  # NEW: auto-injected
    assert "errors" in TestError.__annotations__

    # Should NOT have these fields
    assert "updated_fields" not in TestError.__annotations__  # REMOVED
    assert "id" not in TestError.__annotations__  # REMOVED

    print("✅ Error type annotations correct")


def test_graphql_fields():
    """Verify GraphQL field registration."""
    print("\n=== GraphQL Fields ===")

    success_gql_fields = TestSuccess.__gql_fields__
    error_gql_fields = TestError.__gql_fields__

    print(f"Success GQL fields: {list(success_gql_fields.keys())}")
    print(f"Error GQL fields: {list(error_gql_fields.keys())}")

    # Success fields
    assert "status" in success_gql_fields
    assert "message" in success_gql_fields
    assert "updated_fields" in success_gql_fields
    assert "id" in success_gql_fields
    assert "errors" not in success_gql_fields

    # Error fields
    assert "status" in error_gql_fields
    assert "message" in error_gql_fields
    assert "code" in error_gql_fields
    assert "errors" in error_gql_fields
    assert "updated_fields" not in error_gql_fields
    assert "id" not in error_gql_fields

    print("✅ GraphQL field registration correct")


if __name__ == "__main__":
    test_success_annotations()
    test_error_annotations()
    test_graphql_fields()
    print("\n✅ All manual tests passed!")
```

**Run manual test**:
```bash
cd /home/lionel/code/fraiseql
python manual_test_auto_injection.py
```

**Acceptance Criteria**:
- ✅ All FraiseQL core tests pass
- ✅ All Rust tests pass
- ✅ All integration tests pass
- ✅ Manual test script passes
- ✅ No deprecation warnings
- ✅ No type errors

**Rollback Plan**:
If any test failures:
1. Identify root cause (test incorrect vs implementation incorrect)
2. Fix implementation if needed
3. Update test if expectation was wrong
4. Re-run all tests
5. If unfixable, rollback Phase 1 and Phase 2 changes

---

### Phase 4: PrintOptim Migration

**Objective**: Update PrintOptim codebase to use new auto-injection behavior

**Duration**: 4 hours

**PrintOptim Impact Analysis**:

| Component | Files | Effort |
|-----------|-------|--------|
| **Error Types** | ~45 files | 1.5 hours (AST-based) |
| **Test Queries** | ~138 files | 2 hours (AST-based) |
| **Verification** | All tests | 0.5 hours |

#### Task 4.1: Verify Scope (15 min)

**Count affected files**:
```bash
cd /home/lionel/code/printoptim_backend

# Count Error types with manual code field
echo "Error types with manual code:"
rg "code: int" --type py src/ -c | awk -F: '{s+=$2} END {print s}'

# Count test files referencing updatedFields on Error
echo "Tests with updatedFields on Error:"
rg "updatedFields.*Error|Error.*updatedFields" tests/ --files-with-matches | wc -l

# Count test files referencing id on Error
echo "Tests with id on Error:"
rg "\.\.\. on \w+Error.*\n.*id" tests/ --files-with-matches | wc -l
```

**Expected**: ~45 Error types, ~138 test files

#### Task 4.2: AST-Based Error Type Migration (1 hour)

**Migration script** (`scripts/migrate_error_types_ast.py`):
```python
"""Remove manual code fields from Error types using AST."""

import ast
from pathlib import Path
from typing import Any


class CodeFieldRemover(ast.NodeTransformer):
    """AST transformer that removes 'code: int' from @fraiseql.error classes."""

    def __init__(self):
        self.modified = False

    def visit_ClassDef(self, node: ast.ClassDef) -> Any:
        """Visit class definitions and remove code field if @fraiseql.error decorator."""
        # Check if class has @fraiseql.error or @failure decorator
        has_error_decorator = False
        for decorator in node.decorator_list:
            if isinstance(decorator, ast.Name) and decorator.id in ("error", "failure"):
                has_error_decorator = True
                break
            elif isinstance(decorator, ast.Attribute) and decorator.attr == "error":
                has_error_decorator = True
                break

        if has_error_decorator:
            # Remove AnnAssign nodes with target.id == 'code'
            original_len = len(node.body)
            node.body = [
                stmt
                for stmt in node.body
                if not (
                    isinstance(stmt, ast.AnnAssign)
                    and isinstance(stmt.target, ast.Name)
                    and stmt.target.id == "code"
                )
            ]

            if len(node.body) < original_len:
                self.modified = True
                # If body is now empty, add 'pass'
                if not node.body or all(isinstance(s, ast.Expr) and isinstance(s.value, ast.Constant) for s in node.body):
                    # Only docstring remains or empty - ensure we have pass
                    has_pass = any(isinstance(s, ast.Pass) for s in node.body)
                    if not has_pass:
                        node.body.append(ast.Pass())

        return node


def migrate_error_type_ast(file_path: Path) -> bool:
    """Remove manual code field from Error types using AST.

    Returns:
        True if file was modified, False otherwise
    """
    try:
        content = file_path.read_text()
        tree = ast.parse(content)

        transformer = CodeFieldRemover()
        new_tree = transformer.visit(tree)

        if transformer.modified:
            # Convert AST back to source code
            new_content = ast.unparse(new_tree)
            file_path.write_text(new_content)
            return True

    except SyntaxError as e:
        print(f"⚠️  Syntax error in {file_path}: {e}")
        return False

    return False


def main():
    """Migrate all Error types in PrintOptim."""
    root = Path("/home/lionel/code/printoptim_backend")
    mutation_dir = root / "src/printoptim_backend/entrypoints/api/resolvers/mutation"

    modified_count = 0
    error_count = 0

    print("🔍 Scanning for Error types with manual 'code' field...\n")

    for file_path in mutation_dir.rglob("*.py"):
        if file_path.name.startswith("test_"):
            continue  # Skip test files

        if migrate_error_type_ast(file_path):
            print(f"✅ Migrated: {file_path.relative_to(root)}")
            modified_count += 1

    print(f"\n{'='*60}")
    print(f"✅ Migration complete!")
    print(f"   Files modified: {modified_count}")
    print(f"   Errors: {error_count}")
    print(f"{'='*60}\n")


if __name__ == "__main__":
    main()
```

**Run migration**:
```bash
cd /home/lionel/code/printoptim_backend

# Create scripts directory if needed
mkdir -p scripts

# Copy migration script
# (script content above)

# Run migration
python scripts/migrate_error_types_ast.py

# Verify changes (should show 'code: int' removed)
git diff src/printoptim_backend/entrypoints/api/resolvers/mutation/

# Spot-check a few files
rg "code: int" src/printoptim_backend/entrypoints/api/resolvers/mutation/
# Should return nothing

# Test mutations still work
pytest tests/api/mutations/test_create_machine.py -xvs
pytest tests/api/mutations/test_update_machine.py -xvs
```

**Why AST-based?**
- ✅ Handles multiline docstrings
- ✅ Handles comments between decorator and class
- ✅ Handles type aliases (`code: ErrorCode`)
- ✅ Adds `pass` when class body becomes empty
- ✅ Preserves formatting for non-code fields
- ✅ Safer than regex (syntax-aware)

#### Task 4.3: AST-Based Test Migration (1.5 hours)

**Tests no longer need to check for `errors` on Success** (already done in PrintOptim)

**Tests should NOT check for `updatedFields` or `id` on Error responses**:

**Search for affected tests**:
```bash
cd /home/lionel/code/printoptim_backend

# Find tests checking updatedFields on Error
rg "updatedFields" tests/api/mutations/ | grep -i error

# Find tests checking id on Error
rg 'Error.*\n.*id' tests/api/mutations/
```

**Migration pattern for Error response tests**:

**Before**:
```python
async def test_create_machine_error(graphql_client):
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineError {
                    code
                    status
                    message
                    errors { identifier message }
                    updatedFields  # ❌ Remove
                    id             # ❌ Remove
                }
            }
        }
    """
```

**After**:
```python
async def test_create_machine_error(graphql_client):
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineError {
                    code           # ✅ Still here (now auto-injected)
                    status
                    message
                    errors { identifier message }
                    # updatedFields removed (errors don't update)
                    # id removed (errors don't create entities)
                }
            }
        }
    """
```

**Automated test migration script** (`scripts/migrate_error_tests.py`):
```python
"""Remove updatedFields and id from Error response queries in tests."""

import re
from pathlib import Path


def migrate_test_file(file_path: Path) -> tuple[bool, list[str]]:
    """Remove updatedFields and id from Error fragments in test file.

    Returns:
        (modified: bool, changes: list[str])
    """
    content = file_path.read_text()
    original = content
    changes = []

    # Pattern 1: Remove 'updatedFields' from Error fragments (single line)
    # Matches:   updatedFields
    # Or:        updatedFields  # comment
    pattern = r'(\.\.\. on \w+Error \{[^}]*?)\s+updatedFields(?:\s+#[^\n]*)?\n'
    matches = re.findall(pattern, content, flags=re.DOTALL)
    if matches:
        content = re.sub(pattern, r'\1\n', content, flags=re.DOTALL)
        changes.append(f"Removed updatedFields from {len(matches)} Error fragment(s)")

    # Pattern 2: Remove 'id' from Error fragments (single line, not 'identifier')
    # Matches:   id
    # Or:        id  # comment
    # Not:       identifier (errors field)
    pattern = r'(\.\.\. on \w+Error \{[^}]*?)\s+id(?!entifier)(?:\s+#[^\n]*)?\n'
    matches = re.findall(pattern, content, flags=re.DOTALL)
    if matches:
        content = re.sub(pattern, r'\1\n', content, flags=re.DOTALL)
        changes.append(f"Removed id from {len(matches)} Error fragment(s)")

    # Pattern 3: Remove assertion checks for updatedFields in error responses
    pattern = r'\s+assert\s+"updatedFields"\s+in\s+\w*error\w*.*\n'
    matches = re.findall(pattern, content, flags=re.IGNORECASE)
    if matches:
        content = re.sub(pattern, '', content, flags=re.IGNORECASE)
        changes.append(f"Removed {len(matches)} updatedFields assertion(s)")

    # Pattern 4: Remove assertion checks for id in error responses
    pattern = r'\s+assert\s+"id"\s+in\s+\w*error\w*.*\n'
    matches = re.findall(pattern, content, flags=re.IGNORECASE)
    if matches:
        content = re.sub(pattern, '', content, flags=re.IGNORECASE)
        changes.append(f"Removed {len(matches)} id assertion(s)")

    # Pattern 5: Remove assertions checking updatedFields/id values
    pattern = r'\s+assert\s+\w*error\w*\["updatedFields"\].*\n'
    matches = re.findall(pattern, content, flags=re.IGNORECASE)
    if matches:
        content = re.sub(pattern, '', content, flags=re.IGNORECASE)
        changes.append(f"Removed {len(matches)} updatedFields value check(s)")

    pattern = r'\s+assert\s+\w*error\w*\["id"\].*\n'
    matches = re.findall(pattern, content, flags=re.IGNORECASE)
    if matches:
        content = re.sub(pattern, '', content, flags=re.IGNORECASE)
        changes.append(f"Removed {len(matches)} id value check(s)")

    if content != original:
        file_path.write_text(content)
        return True, changes

    return False, []


def main():
    """Migrate all mutation tests in PrintOptim."""
    root = Path("/home/lionel/code/printoptim_backend")
    test_dir = root / "tests/api/mutations"

    modified_count = 0
    total_changes = []

    print("🔍 Scanning test files for Error fragments with updatedFields/id...\n")

    for file_path in test_dir.rglob("test_*.py"):
        modified, changes = migrate_test_file(file_path)
        if modified:
            print(f"✅ {file_path.relative_to(root)}")
            for change in changes:
                print(f"   - {change}")
            modified_count += 1
            total_changes.extend(changes)

    print(f"\n{'='*60}")
    print(f"✅ Migration complete!")
    print(f"   Files modified: {modified_count}")
    print(f"   Total changes: {len(total_changes)}")
    print(f"{'='*60}\n")


if __name__ == "__main__":
    main()
```

**Run migration**:
```bash
cd /home/lionel/code/printoptim_backend

# Run migration
python scripts/migrate_error_tests.py

# Verify changes
git diff tests/api/mutations/

# Spot-check: Should return nothing
rg "updatedFields" tests/api/mutations/ | grep -i "on.*Error"
rg "\.\.\. on \w+Error.*id[^e]" tests/api/mutations/  # id but not identifier

# Run affected tests
pytest tests/api/mutations/ -x --tb=short -v
```

#### Task 4.4: Verification & Rollback Safety (30 min)

```bash
cd /home/lionel/code/printoptim_backend

# 1. Verify no manual code fields remain
echo "Checking for manual 'code' fields in Error types:"
rg "code: int" src/printoptim_backend/entrypoints/api/resolvers/mutation/
# Should return nothing

# 2. Verify no updatedFields/id in Error fragments
echo "Checking for updatedFields in Error fragments:"
rg "updatedFields" tests/api/mutations/ | grep -i "on.*Error"
# Should return nothing

echo "Checking for id in Error fragments:"
rg "\.\.\. on \w+Error.*id[^e]" tests/api/mutations/
# Should return nothing

# 3. Run mutation tests
pytest tests/api/mutations/ -x --tb=short

# 4. Run integration tests
pytest tests/integration/ -x --tb=short

# 5. Quick sanity check on a few mutations
pytest tests/api/mutations/test_create_machine.py::test_create_machine_validation_error -xvs
pytest tests/api/mutations/test_update_machine.py::test_update_machine_not_found -xvs
```

**Acceptance Criteria**:
- ✅ All Error types have `code` field removed (now auto-injected)
- ✅ All test queries updated (no `updatedFields`/`id` on Error fragments)
- ✅ All PrintOptim mutation tests pass
- ✅ All integration tests pass
- ✅ No manual `code` field definitions remaining
- ✅ Error responses still include `code` (auto-injected by FraiseQL)
- ✅ Error responses do NOT include `updatedFields` or `id`

**Rollback Plan**:
```bash
# If migration fails, revert PrintOptim changes
cd /home/lionel/code/printoptim_backend
git checkout HEAD -- src/printoptim_backend/entrypoints/api/resolvers/mutation/
git checkout HEAD -- tests/api/mutations/

# Verify rollback
pytest tests/api/mutations/ -x
```

**DO NOT**:
- ❌ Modify database functions (out of scope)
- ❌ Change Success type definitions (already correct)
- ❌ Remove `status`, `message`, or `errors` from Error types
- ❌ Modify non-mutation code

---

### Phase 5: Documentation & Release

**Objective**: Update changelog and commit changes

**Duration**: 1-2 hours

**Tasks**:

#### Task 5.1: Update CHANGELOG.md (30 min)

**Add to**: `CHANGELOG.md`

```markdown
## [1.8.1] - 2025-12-XX

### Features

**Auto-inject `code` field on Error types**
- `code` field is now automatically injected on all Error types
- Remove manual `code: int` definitions (no longer needed)
- `code` value is computed from `status` field (422, 404, 409, 500)

**Named fragment support in field extraction**
- Field extraction now supports both inline fragments (`... on Type`) and named fragments (`...FragmentName`)
- Fixes field selection reliability issues

### Breaking Changes

**Remove `updated_fields` from Error types**
- Error types no longer have `updated_fields` field auto-injected
- Rationale: Errors represent failed operations - nothing was updated
- Action: Remove `updatedFields` from Error type GraphQL queries

**Remove `id` from Error types**
- Error types no longer have `id` field auto-injected
- Rationale: Errors represent failed operations - nothing was created
- Action: Remove `id` from Error type GraphQL queries

### Improvements

- Conditional diagnostic logging via `FRAISEQL_DEBUG_FIELD_EXTRACTION` env var
- Cleaner Rust diagnostic logging (wrapped in `#[cfg(debug_assertions)]`)
- Removed dead code in Success response builder (`errors` field handling)

### Migration

**Error Type Definitions**:
```python
# Before (v1.8.0):
@fraiseql.error
class CreateMachineError:
    code: int  # Remove this

# After (v1.8.1):
@fraiseql.error
class CreateMachineError:
    pass  # code auto-injected
```

**GraphQL Queries**:
```graphql
# Before:
... on CreateMachineError {
  code
  status
  message
  updatedFields  # Remove
  id             # Remove
}

# After:
... on CreateMachineError {
  code    # Still available (auto-injected)
  status
  message
}
```

Use AST-based migration scripts in `.phases/fraiseql-auto-injection-redesign/IMPLEMENTATION_PLAN.md` for automated migration.
```

#### Task 5.2: Commit Changes (30 min)

```bash
cd /home/lionel/code/fraiseql

# Stage all changes
git add src/fraiseql/mutations/decorators.py
git add src/fraiseql/mutations/mutation_decorator.py
git add fraiseql_rs/src/mutation/response_builder.rs
git add tests/mutations/
git add CHANGELOG.md

# Commit with descriptive message
git commit -m "$(cat <<'EOF'
feat(mutations): improve error type auto-injection and field extraction [v1.8.1]

**Auto-injection improvements**:
- Auto-inject 'code' field on Error types (remove manual requirement)
- Remove 'updated_fields' from Error types (semantically incorrect)
- Remove 'id' from Error types (semantically incorrect)

**Field extraction fixes**:
- Add named fragment support (fixes reliability issues)
- Add conditional diagnostic logging (FRAISEQL_DEBUG_FIELD_EXTRACTION)

**Rust cleanup**:
- Remove dead code for 'errors' field on Success responses
- Wrap diagnostic logging in #[cfg(debug_assertions)]

**Testing**:
- Add canary tests to prevent future regressions
- Update all decorator and schema tests
- AST-based migration scripts for PrintOptim

**BREAKING CHANGES**:
- Error types no longer have 'updated_fields' or 'id' fields
- GraphQL queries must remove these fields from Error fragments
- 'code' field now auto-injected (remove manual definitions)

Fixes: Field extraction with named fragments
Closes: Auto-injection semantic correctness
EOF
)"

# Verify commit
git show --stat

# Tag release
git tag -a v1.8.1 -m "FraiseQL v1.8.1 - Auto-injection improvements"
```

#### Task 5.3: Update PrintOptim (Optional)

If PrintOptim is in a separate repo, update its dependency:

```bash
cd /home/lionel/code/printoptim_backend

# Update FraiseQL version
uv add "fraiseql@1.8.1"  # or update pyproject.toml

# Commit PrintOptim changes (after migration)
git commit -am "chore: upgrade FraiseQL to v1.8.1"
```

**Acceptance Criteria**:
- ✅ CHANGELOG.md updated with all changes
- ✅ Changes committed with descriptive message
- ✅ Version tagged (v1.8.1)
- ✅ All tests passing
- ✅ No uncommitted changes remaining

**Deliverables**:
- Updated `CHANGELOG.md`
- Git commit with comprehensive message
- Git tag `v1.8.1`
- Clean working directory

---
## 🎯 Final Verification Checklist

Before declaring implementation complete, verify:

### FraiseQL Core

- [ ] Named fragment support implemented (field extraction fix)
- [ ] `code` field auto-injected on Error types
- [ ] `updated_fields` NOT injected on Error types
- [ ] `id` NOT injected on Error types
- [ ] Manual `code` validation removed
- [ ] Success response doesn't include `errors` field in Rust
- [ ] Canary tests added and passing
- [ ] All FraiseQL tests pass
- [ ] All Rust tests pass
- [ ] Conditional diagnostic logging functional

### PrintOptim

- [ ] All Error types have manual `code` removed (AST-based migration)
- [ ] All test queries updated (no `updatedFields`/`id` on Error)
- [ ] All mutation tests pass
- [ ] All integration tests pass
- [ ] No schema validation errors

### Documentation

- [ ] CHANGELOG.md updated with v1.8.1 changes
- [ ] Changes committed with descriptive message
- [ ] Git tag v1.8.1 created

### Verification

- [ ] No uncommitted changes
- [ ] All tests passing in both repos
- [ ] Migration scripts tested and documented

---

## 🚀 Completion Steps

This is a minor release (v1.8.1) - no major release ceremony needed.

### Final Steps

```bash
cd /home/lionel/code/fraiseql

# 1. Run full test suite one last time
pytest tests/ -x
cargo test

# 2. Verify no uncommitted changes
git status

# 3. Merge to dev branch
git checkout dev
git merge feature/post-v1.8.0-improvements

# 4. Tag release
git tag -a v1.8.1 -m "FraiseQL v1.8.1 - Auto-injection improvements"

# 5. Push changes
git push origin dev
git push origin v1.8.1

# Done!
```

### Post-Implementation

1. **Monitor PrintOptim** - Ensure no issues after migration
2. **Document learnings** - Update architecture docs if needed
3. **Close related issues** - Mark field extraction issues as resolved

---

## 📊 Risk Assessment & Mitigation

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Field extraction returns `None` | Medium | High | Phase 0 diagnostic logging, comprehensive testing |
| Rust response builder issues | Low | High | Extensive Rust tests, rollback plan |
| PrintOptim test failures | Medium | Medium | Phased migration, automated scripts |
| Breaking external users | Low | High | Clear migration guide, semantic versioning |

### Migration Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Incomplete Error type migration | Medium | Medium | Automated migration scripts, grep verification |
| Test query updates missed | Medium | Low | Automated test migration, full test suite |
| Documentation gaps | Low | Low | Comprehensive doc review, examples |

### Rollback Strategy

**If critical issues found after release**:

1. **Immediate** - Tag and release v2.0.1 with fixes
2. **Short-term** - Maintain v1.9.x branch for critical patches
3. **Communication** - Notify users of issues and timeline

**Rollback procedure**:
```bash
# In PrintOptim
cd /home/lionel/code/printoptim_backend
uv pip install "fraiseql==1.9.0"
git checkout HEAD~1 -- src/ tests/  # Revert migration changes
pytest tests/ -x  # Verify rollback works

# In FraiseQL
cd /home/lionel/code/fraiseql
git revert v2.0.0  # Revert release commit
git tag -d v2.0.0  # Remove tag
```

---

## 💡 Implementation Best Practices

### During Implementation

1. **Work in phases** - Complete one phase fully before starting next
2. **Test continuously** - Run tests after each change
3. **Commit frequently** - Small, atomic commits with clear messages
4. **Document as you go** - Update docs immediately when code changes
5. **Communicate progress** - Update stakeholders after each phase

### Code Quality

1. **Type hints** - Ensure all Python code has proper type annotations
2. **Docstrings** - Update docstrings for modified functions
3. **Comments** - Explain "why" not "what" for complex logic
4. **Linting** - Run `ruff` before committing
5. **Formatting** - Use consistent style (black/ruff format)

### Testing

1. **Unit tests** - Test individual components
2. **Integration tests** - Test layer interactions
3. **End-to-end tests** - Test full mutation flow
4. **Regression tests** - Ensure no old bugs resurface
5. **Performance tests** - Verify no performance degradation

---

## 📞 Support & Questions

If you encounter issues during implementation:

1. **Check diagnostic logs** - Phase 0 logging provides detailed info
2. **Review phase acceptance criteria** - Ensure all criteria met
3. **Consult analysis documents**:
   - `/tmp/fraiseql-mutation-architecture-analysis.md`
   - `/tmp/fraiseql-auto-injection-comprehensive-analysis.md`
   - `/tmp/fraiseql-bug-response-field-selection-analysis.md`
4. **Rollback if needed** - Each phase has rollback plan

---

## 🎉 Success Criteria

Implementation is successful when:

### Functional Requirements

- ✅ Error types have `code` auto-injected (no manual definition needed)
- ✅ Error types do NOT have `updated_fields` or `id`
- ✅ Success types unchanged (already correct)
- ✅ Rust response builder cleaned up (no `errors` on Success)
- ✅ All FraiseQL tests pass
- ✅ All PrintOptim tests pass

### Non-Functional Requirements

- ✅ Developer experience improved (less boilerplate)
- ✅ Semantic correctness achieved (errors don't have update fields)
- ✅ GraphQL best practices followed
- ✅ Performance maintained or improved
- ✅ Clear migration path provided
- ✅ Comprehensive documentation

### Quality Metrics

- ✅ Test coverage ≥ 90% for modified code
- ✅ Zero breaking changes to Success types
- ✅ All migration scripts tested and verified
- ✅ Documentation review complete
- ✅ Stakeholder sign-off received

---

## 📚 Appendix

### A. Field Auto-Injection Reference

#### Success Types (Final State)

| Field | Type | Auto-Injected? | Conditional? | Rationale |
|-------|------|----------------|--------------|-----------|
| `status` | `str` | ✅ Yes | No | Always "success" |
| `message` | `str \| None` | ✅ Yes | No | Success message |
| `updated_fields` | `list[str] \| None` | ✅ Yes | No | Audit trail |
| `id` | `str \| None` | ✅ Yes | Yes (has entity) | Convenience |
| `errors` | — | ❌ No | — | Removed v1.9.0 |
| `code` | — | ❌ No | — | Only on errors |

#### Error Types (Final State)

| Field | Type | Auto-Injected? | Conditional? | Rationale |
|-------|------|----------------|--------------|-----------|
| `status` | `str` | ✅ Yes | No | Error status |
| `message` | `str` | ✅ Yes | No | Error message |
| `code` | `int` | ✅ **NEW** Yes | No | HTTP-like code |
| `errors` | `list[Error] \| None` | ✅ Yes | No | Field-level errors |
| `updated_fields` | — | ❌ **REMOVED** | — | Not semantic |
| `id` | — | ❌ **REMOVED** | — | Not semantic |

### B. Status-to-Code Mapping

| Status Pattern | Code | HTTP Equivalent |
|----------------|------|-----------------|
| `noop:*` | 422 | Unprocessable Entity |
| `failed:not_found:*` | 404 | Not Found |
| `failed:unauthorized:*` | 401 | Unauthorized |
| `failed:forbidden:*` | 403 | Forbidden |
| `failed:conflict:*` | 409 | Conflict |
| `failed:validation:*` | 422 | Unprocessable Entity |
| `failed:timeout:*` | 408 | Request Timeout |
| `failed:*` (other) | 500 | Internal Server Error |

### C. Database Layer (No Changes)

The `app.mutation_response` composite type remains unchanged:

```sql
CREATE TYPE app.mutation_response AS (
    status          TEXT,
    message         TEXT,
    entity_id       TEXT,
    entity_type     TEXT,
    entity          JSONB,
    updated_fields  TEXT[],
    cascade         JSONB,
    metadata        JSONB
);
```

**No changes to database layer** - all changes are in Python and Rust layers.

---

**Prepared by**: FraiseQL Architecture Team
**Date**: 2025-12-11
**Status**: Ready for Implementation
**Priority**: High - Technical Debt Cleanup + Greenfield Redesign
**Version**: FraiseQL v2.0.0
