# Implementation Plan: Auto-Flatten mutation_result_v2 Entity Wrapper

## Executive Summary

**Problem**: When using `mutation_result_v2` format with cascade support, the Rust mutation pipeline wraps ALL response fields in an `entity` object. This breaks the GraphQL schema contract and forces clients to query through an unexpected nesting level.

**Solution**: Add entity field flattening logic in the Python layer (post-Rust processing) to automatically flatten `entity` JSONB fields to match the Python-defined Success type schema.

**Approach**: Python Layer (Option A) - Post-Rust processing with type introspection

**Priority**: HIGH - Blocking feature that prevents proper use of mutation_result_v2 with cascade

---

## Problem Analysis

### Current Broken Behavior

```python
# Python type definition
@fraiseql.type
class CreatePostSuccess:
    post: Post
    message: str
    cascade: Cascade
```

```graphql
# Client query (what user expects to write)
mutation {
  createPost(input: {...}) {
    ... on CreatePostSuccess {
      post { id, title }    # ← KeyError: 'post' not found
      message               # ← KeyError: 'message' not found
      cascade { updated }   # ← KeyError: 'cascade' not found
    }
  }
}
```

```json
// Actual response (WRONG - has entity wrapper)
{
  "createPost": {
    "__typename": "CreatePostSuccess",
    "entity": {              // ← UNWANTED WRAPPER
      "post": {...},
      "message": "...",
      "cascade": {...}
    }
  }
}
```

### Expected Correct Behavior

```json
// Expected response (CORRECT - fields flattened)
{
  "createPost": {
    "__typename": "CreatePostSuccess",
    "post": {...},           // ← Flattened from entity.post
    "message": "...",        // ← Flattened from entity.message
    "cascade": {...}         // ← Flattened from entity.cascade
  }
}
```

### Root Cause

1. PostgreSQL `mutation_result_v2` returns all custom fields in `entity` JSONB
2. Rust layer correctly extracts and returns them as-is (zero Python parsing)
3. **Missing step**: Python layer should flatten `entity` fields to match Success type schema
4. Result: Client receives `entity.post` when schema says `post`

---

## Architecture Decision

### Chosen Approach: Option A - Python Layer Post-Processing

**Why Python Layer:**
- ✅ Can inspect Python type definitions (Success class annotations)
- ✅ No changes needed to Rust code (zero risk to Rust pipeline)
- ✅ Easier to test with existing Python test infrastructure
- ✅ Can handle type inspection and field mapping
- ✅ Can be conditional based on Success type structure

**Where to implement:**
- **File**: `src/fraiseql/mutations/rust_executor.py`
- **Function**: `execute_mutation_rust()` (after Rust returns response)
- **OR**: New function `_flatten_entity_wrapper()` called from `execute_mutation_rust()`

**Implementation Location Options:**

1. **Inside `rust_executor.py`** (RECOMMENDED):
   - After line 150 (after `mutation_json` is created)
   - Before line 151 (before passing to Rust's `build_mutation_response`)
   - Flatten the `mutation_result` dict before JSON serialization

2. **Alternative - After Rust returns**:
   - After line 119 (after `RustResponseBytes` created)
   - Would require parsing the response bytes (inefficient)
   - **NOT RECOMMENDED** - defeats purpose of Rust-first approach

**Verdict**: Implement flattening at line 150 in `rust_executor.py`, modifying `mutation_result` dict before passing to Rust's `build_mutation_response`.

---

## Pre-Implementation Reconnaissance (COMPLETED)

This section documents the reconnaissance performed before implementation.

### Reconnaissance Results

#### 1. ✅ Caller Search Results
**Command**: `grep -rn "execute_mutation_rust" src/fraiseql --include="*.py"`

**Findings**:
- **SINGLE CALLER FOUND**: `src/fraiseql/mutations/mutation_decorator.py:189`
- Caller context: `MutationResolver.__call__()` method
- Good news: `self.success_type` (the Python class) is already available at line 162
- No test files directly call `execute_mutation_rust` - they go through decorator

**Impact on Plan**: Task 5 simplified - only one file to modify!

#### 2. ✅ Success Type Patterns Survey
**Command**: `grep -rn "class.*Success:" src/fraiseql tests --include="*.py" | head -20`

**Findings**:
- Multiple Success type patterns found in tests
- Common patterns:
  - Minimal: Only `message: str` field
  - Explicit fields: `post: Post`, `message: str`, `cascade: Cascade`
  - Entity-based: Field matching entity type (e.g., `user: User`)
- All patterns have `__annotations__` attribute (can use for introspection)

**Impact on Plan**: `should_flatten_entity()` logic confirmed correct.

#### 3. ✅ Cascade Usage Patterns
**Command**: `grep -r "cascade" tests/integration --include="*.py" -B 2 -A 3`

**Findings**:
- Cascade used in: `test_graphql_cascade.py` (main integration test)
- Current test uses workaround: Accesses through `entity` wrapper (lines 77-81)
- Expected structure after fix: Direct access without `.entity` nesting
- Test fixture in: `tests/fixtures/cascade/conftest.py`

**Impact on Plan**: Task 6 must revert the workaround in lines 77-81.

#### 4. ✅ Test Schema Setup
**Command**: `grep -rn "mutation_result_v2" tests/fixtures --include="*.py"`

**Findings**:
- ✅ `mutation_result_v2` type defined in: `tests/fixtures/cascade/conftest.py:85-88`
- Function using v2: `cascade_db_schema` fixture (lines 79-228)
- Schema creates: `posts` table, `users` table, `create_post` function
- All test infrastructure already in place!

**Impact on Plan**: No test fixture changes needed, can start implementation immediately.

#### 5. ✅ Cascade Parameter in execute_mutation_rust
**Finding**: `cascade_selections: str | None = None` parameter exists (line 38)
**Status**: Already supported, no changes needed

---

## Implementation Design

### Phase 1: Type Introspection Helper

**File**: `src/fraiseql/mutations/entity_flattener.py` (NEW FILE)

**Purpose**: Inspect Success type to determine which fields should be flattened from `entity`

```python
"""Entity field flattening for mutation_result_v2 format."""

from typing import Any, Type, Optional
import logging

logger = logging.getLogger(__name__)


def should_flatten_entity(success_type: Type) -> bool:
    """Determine if Success type has explicit fields requiring entity flattening.

    Returns True if:
    - Success type has explicit field annotations
    - Has fields beyond just 'message' (indicates custom fields)

    Returns False if:
    - Success type has no annotations (generic type)
    - Only has 'message' field (minimal type)
    """
    if not hasattr(success_type, "__annotations__"):
        return False

    annotations = success_type.__annotations__

    # No annotations = generic success type
    if not annotations:
        return False

    # Only 'message' field = minimal success type, no flattening needed
    if set(annotations.keys()) == {"message"}:
        return False

    # Has explicit fields = flatten entity wrapper
    return True


def get_success_type_fields(success_type: Type) -> set[str]:
    """Get field names from Success type annotations.

    Returns set of field names that should exist at top level.
    """
    if not hasattr(success_type, "__annotations__"):
        return set()

    return set(success_type.__annotations__.keys())


def flatten_entity_wrapper(
    mutation_result: dict[str, Any],
    success_type: Type,
) -> dict[str, Any]:
    """Flatten entity JSONB fields to match Success type schema.

    Args:
        mutation_result: Raw mutation result from PostgreSQL (as dict)
        success_type: Python Success type class with field annotations

    Returns:
        Flattened mutation result with entity fields at top level

    Examples:
        # Before flattening
        {
            "status": "created",
            "message": "Success",
            "entity": {"post": {...}, "extra": "data"},
            "cascade": {...},
            "entity_type": "Post",
            "entity_id": "123"
        }

        # After flattening (Success type has 'post', 'message', 'cascade' fields)
        {
            "status": "created",
            "message": "Success",
            "post": {...},      # from entity.post
            "cascade": {...},   # kept from top-level
            "entity_type": "Post",
            "entity_id": "123"
        }
    """
    # Check if this is mutation_result_v2 format
    if "entity" not in mutation_result:
        logger.debug("No entity field found - not v2 format, skipping flattening")
        return mutation_result

    # Check if entity is a dict (JSONB object)
    entity = mutation_result.get("entity")
    if not isinstance(entity, dict):
        logger.debug(f"Entity is not a dict (type: {type(entity)}), skipping flattening")
        return mutation_result

    # Check if Success type has explicit fields
    if not should_flatten_entity(success_type):
        logger.debug(f"Success type {success_type.__name__} has no explicit fields, keeping entity wrapper")
        return mutation_result

    # Get expected field names from Success type
    expected_fields = get_success_type_fields(success_type)

    logger.debug(f"Flattening entity fields for {success_type.__name__}")
    logger.debug(f"Expected fields: {expected_fields}")
    logger.debug(f"Entity keys: {entity.keys()}")

    # Create flattened result (copy original dict)
    flattened = mutation_result.copy()

    # Special handling for specific fields
    # - 'cascade' should come from top-level, NOT entity
    # - 'message' can come from either (top-level takes precedence)

    for field_name in expected_fields:
        if field_name == "cascade":
            # Cascade should always come from top-level mutation_result_v2
            # Don't extract from entity even if present
            continue

        # Extract field from entity if present
        if field_name in entity:
            flattened[field_name] = entity[field_name]
            logger.debug(f"Flattened field '{field_name}' from entity")

    # Remove entity wrapper after flattening
    flattened.pop("entity", None)

    logger.debug(f"Flattened result keys: {flattened.keys()}")

    return flattened
```

### Phase 2: Integration in rust_executor.py

**File**: `src/fraiseql/mutations/rust_executor.py`

**Modify**: Lines 150-170 (after `mutation_json` creation, before Rust call)

**Changes needed**:

1. Import the flattening helper at top of file (around line 7):
```python
from fraiseql.mutations.entity_flattener import flatten_entity_wrapper
```

2. Get Success type class reference (need to add this as parameter or lookup from registry)

**CHALLENGE**: How to get Success type class reference in `rust_executor.py`?

**Solution Options**:

**Option 2A: Add `success_type_class` parameter** (RECOMMENDED)
- Add new parameter to `execute_mutation_rust()`: `success_type_class: Type | None = None`
- Caller passes the actual Python class (not just string name)
- Simple, explicit, no registry lookup needed

**Option 2B: Import from registry**
- Use `SchemaRegistry.get_instance()` to lookup type by name
- More complex, requires registry access
- May have circular import issues

**Decision**: Use Option 2A - add explicit parameter

**Modified function signature** (line 28):
```python
async def execute_mutation_rust(
    conn: Any,
    function_name: str,
    input_data: dict[str, Any],
    field_name: str,
    success_type: str,  # String name for GraphQL
    error_type: str,
    entity_field_name: str | None = None,
    entity_type: str | None = None,
    context_args: list[Any] | None = None,
    cascade_selections: str | None = None,
    config: Any | None = None,
    success_type_class: Type | None = None,  # ← NEW PARAMETER
) -> RustResponseBytes:
```

**Modified flattening logic** (insert after line 150, before passing to Rust):

```python
    # ... existing code creates mutation_result dict ...

    # FLATTEN ENTITY WRAPPER if Success type has explicit fields
    if success_type_class is not None and isinstance(mutation_result, dict):
        mutation_result = flatten_entity_wrapper(mutation_result, success_type_class)

    # Convert to JSON for Rust
    mutation_json = json.dumps(mutation_result, separators=(",", ":"), default=str)
```

### Phase 3: Update Callers to Pass Success Type Class

**Files to modify**: Find all callers of `execute_mutation_rust()`

**Search command**:
```bash
grep -r "execute_mutation_rust" src/fraiseql --include="*.py"
```

**Expected callers**:
1. `src/fraiseql/mutations/executor.py` - Main mutation executor
2. Possibly test files

**For each caller**: Add logic to pass `success_type_class` parameter

**Example in `executor.py`** (hypothetical, need to check actual code):
```python
# Before
result = await execute_mutation_rust(
    conn=conn,
    function_name=function_name,
    input_data=input_data,
    field_name=field_name,
    success_type=mutation_class.success.__name__,
    error_type=mutation_class.error.__name__,
    ...
)

# After
result = await execute_mutation_rust(
    conn=conn,
    function_name=function_name,
    input_data=input_data,
    field_name=field_name,
    success_type=mutation_class.success.__name__,
    error_type=mutation_class.error.__name__,
    success_type_class=mutation_class.success,  # ← Pass the class
    ...
)
```

---

## Implementation Steps - Detailed Task Breakdown

### Task 1: Create Entity Flattener Module

**File**: `src/fraiseql/mutations/entity_flattener.py` (NEW)

**Implementation**:
1. Create new file `src/fraiseql/mutations/entity_flattener.py`
2. Copy the complete implementation from Phase 1 design above
3. Includes functions:
   - `should_flatten_entity(success_type: Type) -> bool`
   - `get_success_type_fields(success_type: Type) -> set[str]`
   - `flatten_entity_wrapper(mutation_result: dict, success_type: Type) -> dict`

**Acceptance criteria**:
- File created with all three functions
- Proper docstrings with examples
- Logging added for debugging
- Type hints for all parameters

**Verification**:
```bash
# File exists
ls -la src/fraiseql/mutations/entity_flattener.py

# Has all required functions
grep "def should_flatten_entity" src/fraiseql/mutations/entity_flattener.py
grep "def get_success_type_fields" src/fraiseql/mutations/entity_flattener.py
grep "def flatten_entity_wrapper" src/fraiseql/mutations/entity_flattener.py
```

---

### Task 2: Add Unit Tests for Entity Flattener

**File**: `tests/unit/mutations/test_entity_flattener.py` (NEW)

**Test cases to write**:

1. **Test should_flatten_entity()**:
   - Type with no annotations → False
   - Type with only 'message' field → False
   - Type with explicit fields ('post', 'message', 'cascade') → True

2. **Test get_success_type_fields()**:
   - Returns correct set of field names
   - Handles type with no annotations

3. **Test flatten_entity_wrapper() - Basic flattening**:
   - Input: mutation_result with entity wrapper
   - Success type: CreatePostSuccess with 'post', 'message', 'cascade'
   - Expected: Fields flattened from entity to top-level

4. **Test flatten_entity_wrapper() - No flattening needed**:
   - Success type has only 'message' field
   - Expected: entity wrapper kept

5. **Test flatten_entity_wrapper() - No entity field**:
   - mutation_result without 'entity' field (v1 format)
   - Expected: Returns unchanged

6. **Test flatten_entity_wrapper() - Cascade from top-level**:
   - Entity has 'cascade' field
   - mutation_result also has top-level 'cascade'
   - Expected: Top-level cascade used, NOT entity.cascade

**Implementation**:
```python
"""Unit tests for entity flattening logic."""

import pytest
from fraiseql.mutations.entity_flattener import (
    should_flatten_entity,
    get_success_type_fields,
    flatten_entity_wrapper,
)


class MinimalSuccess:
    """Success type with only message field."""
    message: str


class CreatePostSuccess:
    """Success type with explicit fields."""
    post: dict  # Simplified for testing
    message: str
    cascade: dict


class NoAnnotations:
    """Success type with no annotations."""
    pass


def test_should_flatten_entity_no_annotations():
    """Type with no annotations should not flatten."""
    assert not should_flatten_entity(NoAnnotations)


def test_should_flatten_entity_minimal():
    """Type with only message should not flatten."""
    assert not should_flatten_entity(MinimalSuccess)


def test_should_flatten_entity_explicit_fields():
    """Type with explicit fields should flatten."""
    assert should_flatten_entity(CreatePostSuccess)


def test_get_success_type_fields():
    """Should return correct field names."""
    fields = get_success_type_fields(CreatePostSuccess)
    assert fields == {"post", "message", "cascade"}


def test_flatten_entity_wrapper_basic():
    """Should flatten entity fields to top level."""
    mutation_result = {
        "status": "created",
        "message": "Post created",
        "entity": {
            "post": {"id": "123", "title": "Test"},
            "extra": "data",
        },
        "cascade": {"updated": [], "deleted": []},
        "entity_type": "Post",
        "entity_id": "123",
    }

    flattened = flatten_entity_wrapper(mutation_result, CreatePostSuccess)

    # Entity wrapper should be removed
    assert "entity" not in flattened

    # Fields should be at top level
    assert flattened["post"] == {"id": "123", "title": "Test"}
    assert flattened["message"] == "Post created"

    # Cascade should come from top-level, not entity
    assert flattened["cascade"] == {"updated": [], "deleted": []}

    # Other fields preserved
    assert flattened["entity_type"] == "Post"
    assert flattened["entity_id"] == "123"


def test_flatten_entity_wrapper_minimal_success():
    """Should keep entity wrapper for minimal success type."""
    mutation_result = {
        "status": "success",
        "message": "Done",
        "entity": {"data": "value"},
    }

    flattened = flatten_entity_wrapper(mutation_result, MinimalSuccess)

    # Entity wrapper should be kept
    assert "entity" in flattened
    assert flattened["entity"] == {"data": "value"}


def test_flatten_entity_wrapper_no_entity_field():
    """Should return unchanged if no entity field (v1 format)."""
    mutation_result = {
        "status": "success",
        "message": "Done",
        "object_data": {"id": "123"},
    }

    flattened = flatten_entity_wrapper(mutation_result, CreatePostSuccess)

    # Should be unchanged
    assert flattened == mutation_result


def test_flatten_entity_wrapper_cascade_priority():
    """Top-level cascade should take priority over entity.cascade."""
    mutation_result = {
        "status": "created",
        "message": "Done",
        "entity": {
            "post": {"id": "123"},
            "cascade": {"wrong": "value"},  # Should be ignored
        },
        "cascade": {"correct": "value"},  # Should be used
    }

    flattened = flatten_entity_wrapper(mutation_result, CreatePostSuccess)

    # Top-level cascade should be used
    assert flattened["cascade"] == {"correct": "value"}
    assert "wrong" not in str(flattened["cascade"])
```

**Verification**:
```bash
# Run tests
uv run pytest tests/unit/mutations/test_entity_flattener.py -v
```

---

### Task 3: Modify rust_executor.py - Add Parameter

**File**: `src/fraiseql/mutations/rust_executor.py`

**Changes**:

1. **Add import** (line ~7):
```python
from typing import Any, Type  # Add Type to existing import
from fraiseql.mutations.entity_flattener import flatten_entity_wrapper
```

2. **Modify function signature** (line ~28):
```python
async def execute_mutation_rust(
    conn: Any,
    function_name: str,
    input_data: dict[str, Any],
    field_name: str,
    success_type: str,
    error_type: str,
    entity_field_name: str | None = None,
    entity_type: str | None = None,
    context_args: list[Any] | None = None,
    cascade_selections: str | None = None,
    config: Any | None = None,
    success_type_class: Type | None = None,  # ← ADD THIS
) -> RustResponseBytes:
```

3. **Add docstring for new parameter** (line ~52):
```python
    Args:
        ...existing args...
        success_type_class: Python Success type class for entity flattening.
            If provided, will flatten entity JSONB fields to match Success type schema.
```

**Verification**:
```bash
# Check import added
grep "from fraiseql.mutations.entity_flattener import" src/fraiseql/mutations/rust_executor.py

# Check parameter added
grep "success_type_class" src/fraiseql/mutations/rust_executor.py | head -3
```

---

### Task 4: Modify rust_executor.py - Add Flattening Logic

**File**: `src/fraiseql/mutations/rust_executor.py`

**Location**: After line 150 (after `mutation_result` dict created, before JSON conversion)

**Find this section** (~line 128-150):
```python
    # Handle different result types from psycopg
    if isinstance(mutation_result, dict):
        # psycopg returned a dict (from JSONB or row_to_json composite)
        # Check for mutation_result_v2 format (has 'status' and 'entity' fields)
        if "status" in mutation_result and "entity" in mutation_result:
            # mutation_result_v2 format from row_to_json - pass through as-is
            pass
        elif "object_data" in mutation_result:
            # Legacy composite type format - convert to v2
            mutation_result = {
                "entity_id": str(mutation_result.get("id")) if mutation_result.get("id") else None,
                ...
            }
        mutation_json = json.dumps(mutation_result, separators=(",", ":"), default=str)
```

**Insert flattening logic BEFORE `mutation_json = json.dumps(...)`**:

```python
    # Handle different result types from psycopg
    if isinstance(mutation_result, dict):
        # psycopg returned a dict (from JSONB or row_to_json composite)
        # Check for mutation_result_v2 format (has 'status' and 'entity' fields)
        if "status" in mutation_result and "entity" in mutation_result:
            # mutation_result_v2 format from row_to_json - pass through as-is
            pass
        elif "object_data" in mutation_result:
            # Legacy composite type format - convert to v2
            mutation_result = {
                "entity_id": str(mutation_result.get("id")) if mutation_result.get("id") else None,
                ...
            }

        # ─────────────────────────────────────────────────────────────
        # FLATTEN ENTITY WRAPPER for mutation_result_v2
        # ─────────────────────────────────────────────────────────────
        if success_type_class is not None:
            mutation_result = flatten_entity_wrapper(mutation_result, success_type_class)
        # ─────────────────────────────────────────────────────────────

        mutation_json = json.dumps(mutation_result, separators=(",", ":"), default=str)
```

**Verification**:
```bash
# Check flattening logic added
grep -A 2 "FLATTEN ENTITY WRAPPER" src/fraiseql/mutations/rust_executor.py

# Check it's before JSON conversion
grep -B 5 "mutation_json = json.dumps" src/fraiseql/mutations/rust_executor.py | grep "flatten_entity_wrapper"
```

---

### Task 5: Find and Update Callers of execute_mutation_rust

**Step 5.1: Reconnaissance Results**

**Search command**:
```bash
grep -rn "execute_mutation_rust" src/fraiseql --include="*.py"
```

**Results**:
1. ✅ **FOUND**: `src/fraiseql/mutations/mutation_decorator.py:157` - Import statement
2. ✅ **FOUND**: `src/fraiseql/mutations/mutation_decorator.py:189` - Actual function call (SINGLE CALLER)
3. ✅ **FOUND**: `src/fraiseql/mutations/rust_executor.py:28` - Function definition (no change needed)

**Key Finding**: There is **ONLY ONE** caller of `execute_mutation_rust` in the entire codebase!
- Location: `src/fraiseql/mutations/mutation_decorator.py` line 189
- Context: Inside the `MutationResolver.__call__()` method
- Good news: `self.success_type` is already available (line 162-163)

**Step 5.2: Examine the existing caller** (lines 189-199)

Current code at `mutation_decorator.py:189-199`:
```python
rust_response = await execute_mutation_rust(
    conn=conn,
    function_name=full_function_name,
    input_data=input_data,
    field_name=field_name,
    success_type=success_type_name,
    error_type=error_type_name,
    entity_field_name=self.entity_field_name,
    entity_type=self.entity_type,
    context_args=context_args if context_args else None,
)
```

Where:
- `success_type_name` comes from line 162: `success_type_name = getattr(self.success_type, "__name__", "Success")`
- `self.success_type` is the actual Python class (set in `__init__` from type hints)

**Step 5.3: Update the single caller**

**File**: `src/fraiseql/mutations/mutation_decorator.py`
**Line**: 189-199

**Change from**:
```python
rust_response = await execute_mutation_rust(
    conn=conn,
    function_name=full_function_name,
    input_data=input_data,
    field_name=field_name,
    success_type=success_type_name,
    error_type=error_type_name,
    entity_field_name=self.entity_field_name,
    entity_type=self.entity_type,
    context_args=context_args if context_args else None,
)
```

**Change to**:
```python
rust_response = await execute_mutation_rust(
    conn=conn,
    function_name=full_function_name,
    input_data=input_data,
    field_name=field_name,
    success_type=success_type_name,
    error_type=error_type_name,
    entity_field_name=self.entity_field_name,
    entity_type=self.entity_type,
    context_args=context_args if context_args else None,
    success_type_class=self.success_type,  # ← ADD THIS LINE
)
```

**Verification**:
```bash
# Check caller updated
grep -A 12 "rust_response = await execute_mutation_rust" src/fraiseql/mutations/mutation_decorator.py | grep "success_type_class"
```

**Note**: Tests do NOT directly call `execute_mutation_rust` - they go through the mutation decorator, so no test file updates needed for Task 5.

---

### Task 6: Update Integration Test (test_graphql_cascade.py)

**File**: `tests/integration/test_graphql_cascade.py`

**Changes needed**: Revert the temporary workaround

**Current broken test** (lines 77-78):
```python
# Temporary workaround: access through entity wrapper
assert data["data"]["createPost"]["entity"]["entityId"]
assert data["data"]["createPost"]["entity"]["message"] == "Post created successfully"
```

**Fix to**:
```python
# After flattening, fields should be at top level
assert data["data"]["createPost"]["id"]  # or whatever field name is used
assert data["data"]["createPost"]["message"] == "Post created successfully"
```

**Current broken cascade access** (line 81):
```python
cascade = data["data"]["createPost"]["entity"]["cascade"]
```

**Fix to**:
```python
cascade = data["data"]["createPost"]["cascade"]
```

**Full test expectations** (lines 32-51) - should work as-is now:
```python
mutation_query = """
mutation CreatePost($input: CreatePostInput!) {
    createPost(input: $input) {
        ... on CreatePostSuccess {
            id          # ← Should work now (no .entity needed)
            message     # ← Should work now
            cascade {   # ← Should work now
                updated
                deleted
                invalidations { ... }
                metadata { ... }
            }
        }
    }
}
"""
```

**Verification**:
```bash
# Run the cascade test
uv run pytest tests/integration/test_graphql_cascade.py::test_cascade_end_to_end -v

# Should pass with NO errors
```

---

### Task 7: Add Integration Test for Backward Compatibility

**File**: `tests/integration/test_entity_flattening.py` (NEW)

**Purpose**: Verify that:
1. mutation_result v1 format still works (printoptim compatibility)
2. mutation_result_v2 with generic Success type keeps entity wrapper
3. mutation_result_v2 with explicit Success type flattens correctly

**Test cases**:

```python
"""Integration tests for entity flattening with mutation_result_v2."""

import pytest
from tests.fixtures.database.database_conftest import *

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_v1_format_backward_compatibility(db_connection, clear_registry):
    """Test that mutation_result v1 format (no entity field) still works."""
    # Setup: Create function that returns old v1 format
    await db_connection.execute("""
        CREATE TYPE mutation_result AS (
            id TEXT,
            status TEXT,
            message TEXT,
            object_data JSONB
        );

        CREATE FUNCTION test_v1_mutation(input_data JSONB)
        RETURNS mutation_result AS $$
        BEGIN
            RETURN ROW(
                'user-123',
                'created',
                'User created',
                '{"id": "user-123", "name": "Alice"}'::jsonb
            )::mutation_result;
        END;
        $$ LANGUAGE plpgsql;
    """)

    # Execute mutation using v1 format
    # ... test that it works as before ...

    # Cleanup
    await db_connection.execute("DROP FUNCTION test_v1_mutation; DROP TYPE mutation_result;")


@pytest.mark.asyncio
async def test_v2_minimal_success_keeps_entity(db_connection, clear_registry):
    """Test that v2 with minimal Success type keeps entity wrapper."""
    # Setup: Create function that returns v2 format
    await db_connection.execute("""
        CREATE FUNCTION test_minimal_mutation(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        BEGIN
            RETURN ROW(
                'created',
                'Success',
                '{"data": "value"}'::jsonb,
                NULL::jsonb,
                'GenericEntity',
                'entity-123',
                NULL,
                NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    # Define minimal Success type (only message)
    @fraiseql.type
    class MinimalSuccess:
        message: str

    # Execute mutation
    # ... verify entity wrapper is kept ...


@pytest.mark.asyncio
async def test_v2_explicit_success_flattens_entity(db_connection, clear_registry):
    """Test that v2 with explicit Success type flattens entity."""
    # Setup: Create function that returns v2 format
    await db_connection.execute("""
        CREATE FUNCTION test_explicit_mutation(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        DECLARE
            post_data JSONB;
        BEGIN
            -- Create entity data with nested post
            post_data := jsonb_build_object(
                'post', jsonb_build_object(
                    'id', 'post-123',
                    'title', 'Test Post'
                )
            );

            RETURN ROW(
                'created',
                'Post created successfully',
                post_data,
                '{"updated": [], "deleted": []}'::jsonb,
                'Post',
                'post-123',
                NULL,
                NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    # Define explicit Success type
    @fraiseql.type
    class Post:
        id: str
        title: str

    @fraiseql.type
    class CreatePostSuccess:
        post: Post
        message: str
        cascade: dict  # Simplified

    # Execute mutation
    # ... verify fields are flattened ...
    # ... verify cascade comes from top-level ...
```

**Verification**:
```bash
# Run backward compat tests
uv run pytest tests/integration/test_entity_flattening.py -v
```

---

## Testing Strategy

### Unit Tests (Task 2)
- **Location**: `tests/unit/mutations/test_entity_flattener.py`
- **Coverage**: All functions in entity_flattener.py
- **Mocking**: Use simple dataclasses for type definitions

### Integration Tests (Tasks 6 & 7)
- **Location**:
  - `tests/integration/test_graphql_cascade.py` (existing, update)
  - `tests/integration/test_entity_flattening.py` (new)
- **Database**: Use real PostgreSQL with mutation_result_v2 type
- **Coverage**:
  - mutation_result_v2 with explicit fields → flattening works
  - mutation_result v1 → unchanged (backward compat)
  - mutation_result_v2 with minimal Success → entity kept

### Manual Testing
- Run against printoptim backend to verify no regression
- Test cascade functionality end-to-end with real GraphQL client

---

## Acceptance Criteria

### Functional Requirements
- ✅ Test `test_cascade_end_to_end` passes with ORIGINAL query structure (no `.entity` nesting)
- ✅ Clients can query Success type fields directly as defined in Python
- ✅ `mutation_result v1` format still works (printoptim not affected)
- ✅ All existing cascade tests pass
- ✅ No breaking changes to existing projects using v1 format

### Technical Requirements
- ✅ Entity flattening only happens for mutation_result_v2 format
- ✅ Flattening only happens when Success type has explicit fields
- ✅ Cascade always comes from top-level, never from entity
- ✅ Entity wrapper removed after flattening
- ✅ All other mutation_result_v2 fields preserved (status, entity_type, etc.)

### Code Quality
- ✅ All unit tests pass
- ✅ All integration tests pass
- ✅ Type hints on all new functions
- ✅ Docstrings with examples
- ✅ Logging for debugging

---

## Rollback Plan

If flattening causes issues:

1. **Remove flattening call** in `rust_executor.py`:
   ```python
   # Comment out this line:
   # mutation_result = flatten_entity_wrapper(mutation_result, success_type_class)
   ```

2. **Remove parameter** from function signature:
   ```python
   # Remove success_type_class parameter
   ```

3. **Revert test changes**:
   ```bash
   git checkout tests/integration/test_graphql_cascade.py
   ```

4. **Delete new files**:
   ```bash
   rm src/fraiseql/mutations/entity_flattener.py
   rm tests/unit/mutations/test_entity_flattener.py
   rm tests/integration/test_entity_flattening.py
   ```

---

## Files Summary

### Files to Create (3)
1. `src/fraiseql/mutations/entity_flattener.py` - Flattening logic
2. `tests/unit/mutations/test_entity_flattener.py` - Unit tests
3. `tests/integration/test_entity_flattening.py` - Integration tests

### Files to Modify (3)
1. `src/fraiseql/mutations/rust_executor.py` - Add parameter and flattening call
2. `tests/integration/test_graphql_cascade.py` - Revert temporary workarounds
3. Caller files (TBD based on search results) - Pass success_type_class

---

## Estimated Effort

- **Task 1**: Create entity_flattener.py - 30 minutes
- **Task 2**: Write unit tests - 1 hour
- **Task 3**: Modify rust_executor.py signature - 15 minutes
- **Task 4**: Add flattening logic - 15 minutes
- **Task 5**: Update callers - 30 minutes (depends on number of callers)
- **Task 6**: Fix cascade test - 15 minutes
- **Task 7**: Write integration tests - 1 hour

**Total**: ~3.5 hours for implementation
**Testing**: +1 hour for manual verification

---

## Next Steps After Implementation

1. **Test against printoptim**: Verify no regression in production backend
2. **Update documentation**: Add entity flattening behavior to docs
3. **Consider Rust optimization**: If performance matters, move flattening to Rust layer
4. **Monitor production**: Watch for any unexpected behavior in real workloads

---

## Decision Log

1. **Why Python layer instead of Rust?**
   - Can inspect Python type annotations easily
   - No changes needed to Rust code (zero risk)
   - Easier to test and debug

2. **Why add parameter instead of registry lookup?**
   - Simpler and more explicit
   - No circular import risks
   - Caller already has the type available

3. **Why flatten before JSON serialization?**
   - Mutating dict is cheaper than parsing JSON later
   - Keeps Rust pipeline unchanged
   - Single point of modification

4. **Why keep cascade at top-level?**
   - Cascade is a mutation_result_v2 field, not entity data
   - Consistent with PostgreSQL function contract
   - Prevents confusion about cascade source
