# FraiseQL-RS v0.2 Migration Guide
## Breaking Changes & Phased Migration Plan for Junior Engineers

**Task Owner:** Junior Engineer
**Estimated Time:** 1-2 days (Rust cleanup completed)
**Complexity:** Complex (Phased TDD Approach Required)

---

## ✅ COMPLETED: Rust Implementation Cleanup

The Rust side of the migration has been **completed** on 2025-10-17. The following changes were made:

### What Was Fixed
1. **Cleaned up `fraiseql_rs/src/lib.rs`:**
   - ✅ Removed unused imports (`Arena`, `TransformConfig`, `ZeroCopyTransformer`, `ByteBuf`)
   - ✅ Removed unused `estimate_arena_size()` function
   - ✅ Simplified to only export the v0.2 public API

2. **Fixed Rust test/benchmark files:**
   - ✅ Updated `benches/core_benchmark.rs` - added missing `add_graphql_wrapper` field
   - ✅ Updated `tests/test_zero_copy.rs` - added missing `add_graphql_wrapper` field
   - ✅ Updated `benches/pipeline.rs` - removed old API, now only benchmarks v0.2
   - ✅ Updated `tests/test_performance.rs` - removed old API comparison
   - ✅ Updated `tests/test_new_impl.rs` - removed old API comparison
   - ✅ Updated `benches/memory.rs` - removed old API comparison

### Build Status
```bash
✓ cargo build --lib              # Clean build, no warnings
✓ cargo build --release --lib    # Clean release build
✓ Python module working          # v0.2.0 API functional
```

### Verified Working
```python
import fraiseql_rs
fraiseql_rs.__version__  # "0.2.0"
fraiseql_rs.build_graphql_response(
    json_strings=['{\"user_id\": 1}'],
    field_name="user",
    type_name="User",
    field_paths=None
)
# Returns: b'{"data":{"user":{"__typename":"User","userId":1}}}'
```

**Result:** The Rust library is now clean, builds without warnings, and exports only the v0.2 API.

---

## 📋 Executive Summary

FraiseQL-RS v0.2.0 introduces **breaking changes** that remove legacy APIs in favor of a streamlined zero-copy implementation. This migration consolidates 11 old functions into **4 core functions**, providing better performance and a cleaner API.

**Current Status:** Rust implementation complete ✓ | Python integration pending

### What Changed?

**REMOVED (v0.1.0):**
```python
# ❌ Deprecated - DO NOT USE
fraiseql_rs.transform_json_with_typename()
fraiseql_rs.transform_with_schema()
fraiseql_rs.build_list_response()
fraiseql_rs.build_single_response()
fraiseql_rs.build_empty_array_response()
fraiseql_rs.build_null_response()
fraiseql_rs.build_list_response_with_projection()    # (if existed)
fraiseql_rs.build_single_response_with_projection()  # (if existed)
fraiseql_rs.SchemaRegistry  # Class removed
```

**NEW (v0.2.0):**
```python
# ✅ Active API - Use these only
fraiseql_rs.to_camel_case(s: str) -> str
fraiseql_rs.transform_keys(obj: dict, recursive: bool = False) -> dict
fraiseql_rs.transform_json(json_str: str) -> str
fraiseql_rs.build_graphql_response(
    json_strings: List[str],
    field_name: str,
    type_name: Optional[str],
    field_paths: Optional[List[List[str]]] = None
) -> bytes
```

---

## 🎯 Migration Objectives

### Completed ✅
- ✅ **Rust Implementation:** All Rust code updated to v0.2 API
- ✅ **Rust Build:** Library builds cleanly with no warnings
- ✅ **Rust Tests/Benchmarks:** Updated to use v0.2 API only
- ✅ **Python Module:** `fraiseql_rs` v0.2.0 installable and functional

### Remaining (Python Integration) 🔄
1. ⏳ **Replace** all deprecated API calls with `build_graphql_response()` in Python code
2. ⏳ **Remove** `SchemaRegistry` usage from `rust_transformer.py`
3. ⏳ **Update** `rust_pipeline.py` to use the new API
4. ⏳ **Fix** all Python test files to use v0.2.0 API
5. ⏳ **Verify** all tests pass with the new implementation
6. ⏳ **Document** any behavior changes

**Estimated Time Remaining:** 1-2 days for Python-side migration

---

## 🔍 Files Requiring Migration

### Core Implementation Files (Priority 1)
1. **`src/fraiseql/core/rust_transformer.py`** (Lines 33, 65, 132, 145)
   - Remove `SchemaRegistry` usage
   - Replace `transform_with_schema()` calls

2. **`src/fraiseql/core/rust_pipeline.py`** (Lines 70, 83, 90, 103, 114, 121)
   - Replace `build_list_response()`
   - Replace `build_single_response()`
   - Replace `build_empty_array_response()`
   - Replace `build_null_response()`

### Test Files (Priority 2)
- 38 test files use deprecated APIs (see `tests/integration/rust/` directory)
- Focus on integration tests first, then unit tests

---

## 📊 PHASES - Disciplined TDD Approach

This migration follows a **phased TDD cycle** methodology to ensure zero regressions and proper validation at each step.

### ⚠️ Important: Rust Work Already Complete

**The Rust implementation has been fully migrated to v0.2.** You do NOT need to modify any Rust (`.rs`) files. The phases below focus **exclusively on Python code** (`rust_transformer.py`, `rust_pipeline.py`, and test files).

**What you'll be doing:**
- ✅ Phase 1-4: Python integration only
- ❌ No Rust code changes needed
- ✅ The `fraiseql_rs` module already exports the correct v0.2 API

---

## PHASE 1: Simplify `rust_transformer.py` (Remove SchemaRegistry)

**Objective:** Remove complex schema-based transformation in favor of the new unified API.

### Analysis

**Current Behavior:**
- Uses `SchemaRegistry` to track type schemas
- Calls `registry.transform(json_str, root_type)` for transformation
- Requires explicit type registration before use

**New Behavior:**
- Use `build_graphql_response()` which handles typename injection automatically
- No schema registration needed (simpler!)
- Transformation happens in one call

### 🔴 RED Phase: Write Failing Test

**Goal:** Create a test that validates the NEW behavior (which will fail initially).

```bash
# Create test file
touch tests/unit/core/test_rust_transformer_v2.py
```

**Test Code:**
```python
"""Test rust_transformer.py with fraiseql_rs v0.2.0 API."""

import pytest
import fraiseql_rs


def test_transform_without_schema_registry():
    """Ensure transformation works without SchemaRegistry."""
    json_str = '{"user_id": 1, "first_name": "John"}'

    # NEW API: Direct transformation without registry
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_str],
        field_name="user",
        type_name="User",
        field_paths=None
    )

    result = result_bytes.decode('utf-8')

    # Should contain GraphQL wrapper
    assert '"data"' in result
    assert '"user"' in result

    # Should have camelCase keys
    assert '"userId"' in result
    assert '"firstName"' in result

    # Should have __typename
    assert '"__typename":"User"' in result


def test_transform_json_only_camelcase():
    """Test simple camelCase transformation without typename."""
    json_str = '{"user_name": "Alice", "email_address": "alice@example.com"}'

    # Use transform_json for simple camelCase (no typename)
    result = fraiseql_rs.transform_json(json_str)

    assert '"userName"' in result
    assert '"emailAddress"' in result
    # Should NOT have __typename
    assert '__typename' not in result
```

**Run the test (expect failure):**
```bash
uv run pytest tests/unit/core/test_rust_transformer_v2.py -v
# Expected: PASS (fraiseql_rs v0.2.0 should work!)
```

**✅ Success Criteria:** Test passes with new API (validates v0.2.0 works as expected)

---

### 🟢 GREEN Phase: Implement Minimal Changes

**Goal:** Make `rust_transformer.py` work with the new API.

#### Step 1: Update `RustTransformer` class

**File:** `src/fraiseql/core/rust_transformer.py`

**Changes:**
```python
# BEFORE (Lines 31-35)
def __init__(self):
    """Initialize the Rust transformer."""
    self._registry: fraiseql_rs.SchemaRegistry = fraiseql_rs.SchemaRegistry()
    self._schema: Dict[str, Dict] = {}
    logger.info("fraiseql-rs transformer initialized (required for FraiseQL v1+)")

# AFTER (Simplified!)
def __init__(self):
    """Initialize the Rust transformer."""
    # SchemaRegistry removed in v0.2.0 - transformation now automatic!
    self._type_names: set[str] = set()  # Track registered types for validation
    logger.info("fraiseql-rs v0.2.0 transformer initialized")
```

#### Step 2: Simplify `register_type` method

```python
# BEFORE (Lines 37-67)
def register_type(self, type_class: Type, type_name: Optional[str] = None) -> None:
    """Register a GraphQL type with the Rust transformer."""
    type_name = type_name or type_class.__name__

    # Build field schema from type annotations
    fields = {}
    annotations = getattr(type_class, "__annotations__", {})

    for field_name, field_type in annotations.items():
        if field_name.startswith("_"):
            continue
        schema_type = self._map_python_type_to_schema(field_type)
        if schema_type:
            fields[field_name] = schema_type

    # Register with fraiseql-rs
    type_def = {"fields": fields}
    self._schema[type_name] = type_def
    self._registry.register_type(type_name, type_def)

    logger.debug(f"Registered type '{type_name}' with {len(fields)} fields")

# AFTER (Much simpler!)
def register_type(self, type_class: Type, type_name: Optional[str] = None) -> None:
    """Register a GraphQL type name (schema analysis removed in v0.2.0).

    Note: fraiseql_rs v0.2.0 no longer requires schema registration.
    This method now just tracks type names for validation purposes.
    """
    type_name = type_name or type_class.__name__
    self._type_names.add(type_name)
    logger.debug(f"Registered type '{type_name}' (v0.2.0 - no schema needed)")
```

#### Step 3: Remove `_map_python_type_to_schema` method

```python
# REMOVE entire method (Lines 69-121)
# def _map_python_type_to_schema(self, python_type: Type) -> Optional[str]:
#     ... (delete entire method)
```

#### Step 4: Update `transform` method

```python
# BEFORE (Lines 122-132)
def transform(self, json_str: str, root_type: str) -> str:
    """Transform JSON string using Rust transformer."""
    return self._registry.transform(json_str, root_type)

# AFTER (Use new API)
def transform(self, json_str: str, root_type: str) -> str:
    """Transform JSON string to GraphQL response format.

    Args:
        json_str: JSON string with snake_case keys
        root_type: GraphQL type name for __typename injection

    Returns:
        GraphQL response JSON string with camelCase + __typename
    """
    # v0.2.0: Use build_graphql_response for single object
    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_str],
        field_name="data",  # Generic wrapper field
        type_name=root_type,
        field_paths=None
    )

    # Return as string (bytes → str)
    return response_bytes.decode('utf-8')
```

#### Step 5: Update `transform_json_passthrough` method

```python
# BEFORE (Lines 134-148)
def transform_json_passthrough(self, json_str: str, root_type: Optional[str] = None) -> str:
    """Transform JSON without typename if not needed."""
    if root_type and root_type in self._schema:
        return self._registry.transform(json_str, root_type)
    # Use plain transform_json for camelCase only
    return fraiseql_rs.transform_json(json_str)

# AFTER (Simplified)
def transform_json_passthrough(self, json_str: str, root_type: Optional[str] = None) -> str:
    """Transform JSON to camelCase (optionally with __typename).

    Args:
        json_str: JSON string with snake_case keys
        root_type: Optional type name for __typename injection

    Returns:
        Transformed JSON string with camelCase keys
    """
    if root_type:
        # With typename injection
        response_bytes = fraiseql_rs.build_graphql_response(
            json_strings=[json_str],
            field_name="data",
            type_name=root_type,
            field_paths=None
        )
        return response_bytes.decode('utf-8')
    else:
        # CamelCase only (no typename)
        return fraiseql_rs.transform_json(json_str)
```

**Run tests:**
```bash
uv run pytest tests/unit/core/test_rust_transformer_v2.py -v
# Expected: PASS
```

**✅ Success Criteria:** New test passes with simplified implementation.

---

### 🔧 REFACTOR Phase: Clean Up & Optimize

**Goal:** Remove dead code, improve clarity, ensure consistency.

#### Step 1: Remove unused imports

```python
# Remove if not used elsewhere:
from typing import Optional  # May still be needed
# Remove: Type annotation imports for schema mapping (no longer needed)
```

#### Step 2: Update docstrings

```python
class RustTransformer:
    """Manages fraiseql-rs JSON transformations (v0.2.0+).

    This class provides integration with fraiseql-rs for high-performance
    JSON transformation from snake_case to camelCase with __typename injection.

    Note: SchemaRegistry removed in v0.2.0 - transformation is now automatic.
    """
```

#### Step 3: Run broader test suite

```bash
# Run all rust transformer tests
uv run pytest tests/unit/core/ -k rust -v

# Run integration tests using rust_transformer
uv run pytest tests/integration/rust/ -v
```

**✅ Success Criteria:** All tests pass, no regressions.

---

### ✅ QA Phase: Verify Phase 1 Completion

**Checklist:**
- [ ] All tests in `tests/unit/core/test_rust_transformer_v2.py` pass
- [ ] Original tests still pass (if any)
- [ ] No references to `SchemaRegistry` remain in `rust_transformer.py`
- [ ] Code is cleaner and simpler than before
- [ ] Docstrings updated to reflect v0.2.0 changes
- [ ] `uv run pytest tests/unit/core/ -v` passes fully

**Quality Gates:**
```bash
# All tests pass
uv run pytest tests/unit/core/ --tb=short

# Type checking
uv run mypy src/fraiseql/core/rust_transformer.py

# Code quality
uv run ruff check src/fraiseql/core/rust_transformer.py
```

**🎉 Phase 1 Complete!** Proceed to Phase 2.

---

## PHASE 2: Update `rust_pipeline.py` (Replace Build Functions)

**Objective:** Replace all deprecated `build_*_response()` functions with the unified `build_graphql_response()`.

### Analysis

**Current API Usage:**
```python
# rust_pipeline.py uses these deprecated functions:
fraiseql_rs.build_empty_array_response(field_name)
fraiseql_rs.build_list_response(json_strings, field_name, type_name)
fraiseql_rs.build_list_response_with_projection(...)  # May not exist
fraiseql_rs.build_single_response(json_string, field_name, type_name)
fraiseql_rs.build_single_response_with_projection(...)  # May not exist
fraiseql_rs.build_null_response(field_name)
```

**New Unified API:**
```python
# Single function handles ALL cases:
fraiseql_rs.build_graphql_response(
    json_strings: List[str],      # Always a list (even for single object)
    field_name: str,              # GraphQL field name
    type_name: Optional[str],     # Type for __typename (None = no typename)
    field_paths: Optional[List[List[str]]] = None  # Field projection
) -> bytes
```

### 🔴 RED Phase: Write Failing Tests

**Goal:** Create tests for the NEW API behavior.

```bash
# Create test file
touch tests/unit/core/test_rust_pipeline_v2.py
```

**Test Code:**
```python
"""Test rust_pipeline.py with fraiseql_rs v0.2.0 API."""

import pytest
import fraiseql_rs
from src.fraiseql.core.rust_pipeline import RustResponseBytes


def test_build_graphql_response_list():
    """Test list response with new API."""
    json_strings = [
        '{"id": 1, "user_name": "Alice"}',
        '{"id": 2, "user_name": "Bob"}'
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=json_strings,
        field_name="users",
        type_name="User",
        field_paths=None
    )

    result = response_bytes.decode('utf-8')

    # Should have GraphQL wrapper
    assert '"data"' in result
    assert '"users"' in result

    # Should be an array
    assert '[' in result

    # Should have camelCase
    assert '"userName"' in result

    # Should have __typename
    assert '"__typename":"User"' in result


def test_build_graphql_response_empty_list():
    """Test empty list response."""
    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[],  # Empty list
        field_name="users",
        type_name=None,
        field_paths=None
    )

    result = response_bytes.decode('utf-8')

    # Should have empty array
    assert '"users":[]' in result


def test_build_graphql_response_single_object():
    """Test single object response."""
    json_string = '{"id": 1, "user_name": "Alice"}'

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],  # Single item in list
        field_name="user",
        type_name="User",
        field_paths=None
    )

    result = response_bytes.decode('utf-8')

    # Should NOT be an array (single object)
    assert '"user":{' in result
    assert '"userName":"Alice"' in result


def test_build_graphql_response_with_projection():
    """Test field projection."""
    json_string = '{"id": 1, "user_name": "Alice", "email": "alice@example.com", "age": 30}'

    field_paths = [["id"], ["user_name"]]  # Only request id and user_name

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=field_paths
    )

    result = response_bytes.decode('utf-8')

    # Should have projected fields
    assert '"id"' in result
    assert '"userName"' in result

    # Should NOT have non-projected fields
    assert '"email"' not in result
    assert '"age"' not in result


def test_rust_response_bytes_wrapper():
    """Test RustResponseBytes wrapper class."""
    data = b'{"test": "data"}'
    wrapper = RustResponseBytes(data)

    assert wrapper.bytes == data
    assert wrapper.content_type == "application/json"
    assert bytes(wrapper) == data
```

**Run tests:**
```bash
uv run pytest tests/unit/core/test_rust_pipeline_v2.py -v
# Expected: PASS (validates v0.2.0 API works)
```

**✅ Success Criteria:** All tests pass, confirming new API works correctly.

---

### 🟢 GREEN Phase: Update `rust_pipeline.py`

**Goal:** Replace all deprecated function calls with `build_graphql_response()`.

#### Step 1: Update empty array response

**File:** `src/fraiseql/core/rust_pipeline.py`

```python
# BEFORE (Line 70)
response_bytes = fraiseql_rs.build_empty_array_response(field_name)

# AFTER
response_bytes = fraiseql_rs.build_graphql_response(
    json_strings=[],  # Empty list
    field_name=field_name,
    type_name=None,  # No typename for empty array
    field_paths=None
)
```

#### Step 2: Update list response (with/without projection)

```python
# BEFORE (Lines 82-94)
if field_paths:
    response_bytes = fraiseql_rs.build_list_response_with_projection(
        json_strings,
        field_name,
        type_name,
        field_paths,
    )
else:
    response_bytes = fraiseql_rs.build_list_response(
        json_strings,
        field_name,
        type_name,  # None = no transformation
    )

# AFTER (Single unified call!)
response_bytes = fraiseql_rs.build_graphql_response(
    json_strings=json_strings,
    field_name=field_name,
    type_name=type_name,
    field_paths=field_paths  # None = no projection
)
```

#### Step 3: Update null response

```python
# BEFORE (Line 103)
response_bytes = fraiseql_rs.build_null_response(field_name)

# AFTER
# For null responses, we can use empty list or handle specially
# Option 1: Return empty data structure
response_bytes = fraiseql_rs.build_graphql_response(
    json_strings=[],
    field_name=field_name,
    type_name=None,
    field_paths=None
)

# Option 2: Build null JSON manually (if API doesn't handle it)
# null_json = '{"data": {"' + field_name + '": null}}'
# response_bytes = null_json.encode('utf-8')
```

**⚠️ NOTE:** Test which approach works best. The new API might need adjustment for null handling.

#### Step 4: Update single object response (with/without projection)

```python
# BEFORE (Lines 113-126)
if field_paths:
    response_bytes = fraiseql_rs.build_single_response_with_projection(
        json_string,
        field_name,
        type_name,
        field_paths,
    )
else:
    response_bytes = fraiseql_rs.build_single_response(
        json_string,
        field_name,
        type_name,
    )

# AFTER (Single unified call!)
response_bytes = fraiseql_rs.build_graphql_response(
    json_strings=[json_string],  # Single item in list
    field_name=field_name,
    type_name=type_name,
    field_paths=field_paths  # None = no projection
)
```

#### Step 5: Updated full function

Here's the complete updated `execute_via_rust_pipeline` function:

```python
async def execute_via_rust_pipeline(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: Optional[Dict[str, Any]],
    field_name: str,
    type_name: Optional[str],
    is_list: bool = True,
    field_paths: Optional[List[List[str]]] = None,
) -> RustResponseBytes:
    """Execute query and build HTTP response entirely in Rust.

    This is the FASTEST path: PostgreSQL → Rust → HTTP bytes.
    Zero Python string operations, zero JSON parsing, zero copies.

    Args:
        conn: PostgreSQL connection
        query: SQL query returning JSON strings
        params: Query parameters
        field_name: GraphQL field name (e.g., "users")
        type_name: GraphQL type for transformation (e.g., "User")
        is_list: True for arrays, False for single objects
        field_paths: Optional field paths for projection (e.g., [["id"], ["firstName"]])

    Returns:
        RustResponseBytes ready for HTTP response
    """
    async with conn.cursor() as cursor:
        await cursor.execute(query, params or {})

        if is_list:
            rows = await cursor.fetchall()

            if not rows:
                # Empty array response
                response_bytes = fraiseql_rs.build_graphql_response(
                    json_strings=[],
                    field_name=field_name,
                    type_name=None,  # No typename for empty
                    field_paths=None
                )
                return RustResponseBytes(response_bytes)

            # Extract JSON strings (PostgreSQL returns as text)
            json_strings = [row[0] for row in rows if row[0] is not None]

            # 🚀 UNIFIED API (v0.2.0):
            # - Field projection: Filter only requested fields
            # - Concatenate: ['{"id":"1"}', '{"id":"2"}'] → '[{"id":"1"},{"id":"2"}]'
            # - Wrap: '[...]' → '{"data":{"users":[...]}}'
            # - Transform: snake_case → camelCase + __typename
            # - Encode: String → UTF-8 bytes
            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=json_strings,
                field_name=field_name,
                type_name=type_name,
                field_paths=field_paths
            )

            return RustResponseBytes(response_bytes)
        else:
            # Single object
            row = await cursor.fetchone()

            if not row or row[0] is None:
                # Null response - use empty structure or null
                # TODO: Verify behavior with fraiseql_rs v0.2.0
                response_bytes = fraiseql_rs.build_graphql_response(
                    json_strings=[],
                    field_name=field_name,
                    type_name=None,
                    field_paths=None
                )
                return RustResponseBytes(response_bytes)

            json_string = row[0]

            # 🚀 UNIFIED API (v0.2.0):
            # - Field projection: Filter only requested fields
            # - Wrap: '{"id":"1"}' → '{"data":{"user":{"id":"1"}}}'
            # - Transform: snake_case → camelCase + __typename
            # - Encode: String → UTF-8 bytes
            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=[json_string],  # Single item as list
                field_name=field_name,
                type_name=type_name,
                field_paths=field_paths
            )

            return RustResponseBytes(response_bytes)
```

**Run tests:**
```bash
uv run pytest tests/unit/core/test_rust_pipeline_v2.py -v
# Expected: PASS
```

**✅ Success Criteria:** Tests pass with the new implementation.

---

### 🔧 REFACTOR Phase: Improve Code Quality

#### Step 1: Simplify logic

The new API eliminated the if/else for projection - clean up any dead code:

```python
# Remove conditional logic for with_projection vs without
# The new API handles both cases automatically
```

#### Step 2: Update comments

```python
# Update all comments referencing old API:
# OLD: "Uses build_list_response for transformation"
# NEW: "Uses build_graphql_response unified API (v0.2.0)"
```

#### Step 3: Add validation

```python
# Optional: Add validation for null handling
if is_list and not json_strings:
    logger.debug(f"Empty list response for field '{field_name}'")
```

#### Step 4: Run integration tests

```bash
# Test the full pipeline with real database
uv run pytest tests/integration/graphql/ -v

# Test specific rust pipeline integration
uv run pytest tests/system/ -k rust -v
```

**✅ Success Criteria:** All integration tests pass, code is cleaner.

---

### ✅ QA Phase: Verify Phase 2 Completion

**Checklist:**
- [ ] All deprecated `build_*_response()` calls removed
- [ ] `build_graphql_response()` used for all cases
- [ ] Tests pass: `uv run pytest tests/unit/core/test_rust_pipeline_v2.py -v`
- [ ] Integration tests pass: `uv run pytest tests/integration/ -v`
- [ ] Null handling works correctly
- [ ] Field projection works correctly
- [ ] Performance is same or better than before

**Quality Gates:**
```bash
# All tests pass
uv run pytest tests/ --tb=short

# No deprecated API usage remains
grep -r "build_list_response\|build_single_response\|build_empty_array\|build_null_response" src/fraiseql/core/

# Code quality
uv run ruff check src/fraiseql/core/rust_pipeline.py
uv run mypy src/fraiseql/core/rust_pipeline.py
```

**🎉 Phase 2 Complete!** Proceed to Phase 3.

---

## PHASE 3: Update Test Files (38 files)

**Objective:** Update all test files that use deprecated fraiseql_rs APIs.

### Strategy

Instead of updating all 38 files individually, we'll use a **search-and-replace strategy** with validation.

### 🔴 RED Phase: Identify All Usages

**Goal:** Find every deprecated API usage in tests.

```bash
# Find all deprecated API calls
grep -r "transform_json_with_typename\|transform_with_schema\|SchemaRegistry\|build_list_response\|build_single_response\|build_empty_array\|build_null_response" tests/ > /tmp/deprecated_usage.txt

# Review the output
cat /tmp/deprecated_usage.txt
```

**Document findings:**
- Which files use which deprecated APIs
- What they're testing
- Whether the test is still relevant

### 🟢 GREEN Phase: Update Tests File by File

**Approach:** Update tests in priority order:

1. **Integration tests** (tests/integration/rust/)
2. **Unit tests** (tests/unit/)
3. **System tests** (tests/system/)
4. **Regression tests** (tests/regression/)

#### Example: Update `test_typename_injection.py`

**Before:**
```python
def test_typename_injection():
    json_str = '{"user_id": 1}'
    result = fraiseql_rs.transform_json_with_typename(json_str, "User")
    assert '"__typename":"User"' in result
```

**After:**
```python
def test_typename_injection():
    json_str = '{"user_id": 1}'
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_str],
        field_name="data",
        type_name="User",
        field_paths=None
    )
    result = result_bytes.decode('utf-8')
    assert '"__typename":"User"' in result
```

#### Automated Search-Replace Script

Create a helper script to speed up migration:

**File:** `scripts/migrate_fraiseql_rs_v2.py`

```python
#!/usr/bin/env python3
"""Helper script to migrate fraiseql_rs v0.1 → v0.2 API calls."""

import re
import sys
from pathlib import Path


def migrate_transform_json_with_typename(content: str) -> str:
    """Replace transform_json_with_typename() calls."""
    # Pattern: fraiseql_rs.transform_json_with_typename(json_str, type_name)
    pattern = r'fraiseql_rs\.transform_json_with_typename\(([^,]+),\s*([^)]+)\)'

    def replacement(match):
        json_str = match.group(1)
        type_name = match.group(2)
        return f'''fraiseql_rs.build_graphql_response(
        json_strings=[{json_str}],
        field_name="data",
        type_name={type_name},
        field_paths=None
    ).decode('utf-8')'''

    return re.sub(pattern, replacement, content)


def migrate_schema_registry(content: str) -> str:
    """Remove SchemaRegistry usage."""
    # This is complex - flag for manual review
    if 'SchemaRegistry' in content:
        print("⚠️  WARNING: SchemaRegistry found - requires manual migration!")
    return content


def migrate_file(file_path: Path) -> None:
    """Migrate a single file."""
    print(f"📝 Migrating: {file_path}")

    content = file_path.read_text()
    original_content = content

    # Apply transformations
    content = migrate_transform_json_with_typename(content)
    content = migrate_schema_registry(content)

    # Write back if changed
    if content != original_content:
        file_path.write_text(content)
        print(f"✅ Updated: {file_path}")
    else:
        print(f"⏭️  No changes: {file_path}")


def main():
    """Migrate all test files."""
    test_dir = Path("tests")

    # Find all Python test files
    test_files = list(test_dir.rglob("test_*.py"))

    print(f"Found {len(test_files)} test files")

    for file_path in test_files:
        migrate_file(file_path)

    print("\n✅ Migration complete! Run tests to verify:")
    print("   uv run pytest tests/ -v")


if __name__ == "__main__":
    main()
```

**Run the migration script:**
```bash
chmod +x scripts/migrate_fraiseql_rs_v2.py
python scripts/migrate_fraiseql_rs_v2.py
```

**After running, manually review:**
```bash
# Check what changed
git diff tests/

# Test each changed file
uv run pytest tests/integration/rust/test_typename_injection.py -v
```

### 🔧 REFACTOR Phase: Clean Up Test Code

After migration:

1. **Remove duplicate tests** - Some tests might be redundant with new API
2. **Consolidate test helpers** - Create shared fixtures for common patterns
3. **Update test docstrings** - Reflect v0.2.0 API usage

### ✅ QA Phase: Verify Phase 3 Completion

**Checklist:**
- [ ] All 38 test files reviewed
- [ ] No deprecated API calls remain: `grep -r "transform_json_with_typename" tests/`
- [ ] All tests pass: `uv run pytest tests/ -v`
- [ ] Code coverage maintained or improved
- [ ] Test execution time same or better

**Quality Gates:**
```bash
# Full test suite passes
uv run pytest tests/ --tb=short

# No deprecated usage
grep -r "SchemaRegistry\|transform_json_with_typename\|transform_with_schema" tests/
# Expected: No results

# Coverage check
uv run pytest tests/ --cov=src/fraiseql/core --cov-report=term-missing
```

**🎉 Phase 3 Complete!** Proceed to Phase 4.

---

## PHASE 4: Final Validation & Documentation

**Objective:** Ensure the migration is complete, tested, and documented.

### 🔴 RED Phase: Create End-to-End Test

**Goal:** Create a comprehensive test that validates the entire migration.

```bash
touch tests/integration/test_fraiseql_rs_v2_migration.py
```

**Test Code:**
```python
"""End-to-end test for fraiseql_rs v0.2.0 migration."""

import pytest
import fraiseql_rs
from src.fraiseql.core.rust_transformer import get_transformer
from src.fraiseql.core.rust_pipeline import RustResponseBytes


class User:
    """Example GraphQL type."""
    id: int
    user_name: str
    email: str


def test_full_migration_e2e():
    """Test complete workflow with v0.2.0 API."""

    # 1. Type registration (simplified in v0.2.0)
    transformer = get_transformer()
    transformer.register_type(User, "User")

    # 2. Database JSON transformation
    db_json = '{"id": 1, "user_name": "Alice", "email": "alice@example.com"}'

    # 3. Transform using new API
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[db_json],
        field_name="user",
        type_name="User",
        field_paths=None
    )

    result = result_bytes.decode('utf-8')

    # 4. Validate GraphQL response structure
    assert '"data"' in result
    assert '"user"' in result

    # 5. Validate camelCase transformation
    assert '"userName":"Alice"' in result

    # 6. Validate __typename injection
    assert '"__typename":"User"' in result

    # 7. Validate RustResponseBytes wrapper
    wrapper = RustResponseBytes(result_bytes)
    assert wrapper.content_type == "application/json"
    assert bytes(wrapper) == result_bytes


def test_migration_performance():
    """Ensure v0.2.0 is as fast or faster than v0.1.0."""
    import time

    json_strings = [
        '{"id": %d, "user_name": "User%d"}' % (i, i)
        for i in range(1000)
    ]

    start = time.perf_counter()

    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=json_strings,
        field_name="users",
        type_name="User",
        field_paths=None
    )

    duration = time.perf_counter() - start

    # Should complete in under 10ms for 1000 objects
    assert duration < 0.01, f"Performance regression: {duration:.4f}s"

    # Verify result is valid
    result = result_bytes.decode('utf-8')
    assert '"users":[' in result
```

**Run test:**
```bash
uv run pytest tests/integration/test_fraiseql_rs_v2_migration.py -v
# Expected: PASS
```

### 🟢 GREEN Phase: Update Documentation

#### Step 1: Update main README

**File:** `README.md`

Add migration notice:

```markdown
## ⚠️ Breaking Changes - fraiseql_rs v0.2.0

**If upgrading from fraiseql_rs v0.1.x, see [FRAISEQL_RS_V0.2_MIGRATION_GUIDE.md](./FRAISEQL_RS_V0.2_MIGRATION_GUIDE.md)**

Key changes:
- ✅ Unified API: Single `build_graphql_response()` function
- ❌ Removed: `SchemaRegistry`, `build_list_response()`, `build_single_response()`
- 🚀 Performance: 2.5x faster with zero-copy implementation
```

#### Step 2: Create changelog entry

**File:** `CHANGELOG.md`

```markdown
## [Unreleased] - 2025-XX-XX

### Breaking Changes - fraiseql_rs v0.2.0 Integration

#### Removed
- `fraiseql_rs.SchemaRegistry` class
- `fraiseql_rs.transform_json_with_typename()`
- `fraiseql_rs.transform_with_schema()`
- `fraiseql_rs.build_list_response()`
- `fraiseql_rs.build_single_response()`
- `fraiseql_rs.build_empty_array_response()`
- `fraiseql_rs.build_null_response()`

#### Added
- Unified `fraiseql_rs.build_graphql_response()` API
- Simplified `RustTransformer` (no schema registration needed)
- Updated `rust_pipeline.py` for v0.2.0 compatibility

#### Performance
- 2.5x speedup in JSON transformation (zero-copy SIMD implementation)
- Reduced memory allocations in Rust layer
- Cleaner API with fewer function calls

#### Migration
- See [FRAISEQL_RS_V0.2_MIGRATION_GUIDE.md](./FRAISEQL_RS_V0.2_MIGRATION_GUIDE.md) for step-by-step guide
- All tests updated to use new API
- Backward compatibility: None (breaking change)
```

#### Step 3: Update API documentation

**File:** `docs/api/rust_integration.md` (create if needed)

```markdown
# Rust Integration API (v0.2.0)

## Core Functions

### `build_graphql_response()`

Unified function for building GraphQL HTTP responses from PostgreSQL JSON.

**Signature:**
```python
def build_graphql_response(
    json_strings: List[str],
    field_name: str,
    type_name: Optional[str],
    field_paths: Optional[List[List[str]]] = None
) -> bytes
```

**Arguments:**
- `json_strings` - List of JSON strings from database (even for single object)
- `field_name` - GraphQL field name (e.g., "users", "user")
- `type_name` - Type name for `__typename` injection (None = no typename)
- `field_paths` - Field projection paths (e.g., `[["id"], ["firstName"]]`)

**Returns:**
- `bytes` - UTF-8 encoded GraphQL response ready for HTTP

**Example:**
```python
import fraiseql_rs

# List response
result = fraiseql_rs.build_graphql_response(
    json_strings=['{"user_id": 1}', '{"user_id": 2}'],
    field_name="users",
    type_name="User",
    field_paths=[["user_id"]]  # Only return userId
)

# Output: b'{"data":{"users":[{"__typename":"User","userId":1},{"__typename":"User","userId":2}]}}'
```

### Migration from v0.1.0

**Old API (REMOVED):**
```python
# ❌ Don't use - will fail with ImportError
registry = fraiseql_rs.SchemaRegistry()
result = fraiseql_rs.build_list_response(jsons, "users", "User")
```

**New API (v0.2.0):**
```python
# ✅ Use this instead
result = fraiseql_rs.build_graphql_response(jsons, "users", "User", None)
```
```

### 🔧 REFACTOR Phase: Code Cleanup

#### Step 1: Remove migration helpers

```bash
# Remove temporary migration files if created
rm -f /tmp/deprecated_usage.txt
rm -f scripts/migrate_fraiseql_rs_v2.py
```

#### Step 2: Update type hints

Ensure all type hints are correct:

```python
# rust_pipeline.py
from typing import List, Optional

# Ensure proper typing
field_paths: Optional[List[List[str]]] = None
```

#### Step 3: Run final linting

```bash
# Format code
uv run black src/fraiseql/core/

# Lint
uv run ruff check src/fraiseql/core/ --fix

# Type check
uv run mypy src/fraiseql/core/
```

### ✅ QA Phase: Final Validation

**Full System Test:**
```bash
# 1. Run complete test suite
uv run pytest tests/ -v --tb=short

# 2. Run with coverage
uv run pytest tests/ --cov=src/fraiseql/core --cov-report=html

# 3. Check for deprecated usage
grep -r "SchemaRegistry\|transform_json_with_typename\|build_list_response" src/ tests/
# Expected: No results (except in comments/docs)

# 4. Performance benchmark
uv run pytest tests/integration/test_fraiseql_rs_v2_migration.py::test_migration_performance -v

# 5. Integration tests
uv run pytest tests/integration/ -v

# 6. System tests
uv run pytest tests/system/ -v
```

**Final Checklist:**
- [ ] All tests pass (0 failures)
- [ ] No deprecated API usage in codebase
- [ ] Documentation updated (README, CHANGELOG, API docs)
- [ ] Performance meets or exceeds v0.1.0
- [ ] Code quality checks pass (ruff, mypy, black)
- [ ] Migration guide reviewed and accurate
- [ ] Git commit created with clear message

**Commit Message Template:**
```
feat: Migrate to fraiseql_rs v0.2.0 API

BREAKING CHANGE: fraiseql_rs v0.2.0 removes legacy APIs

- Remove SchemaRegistry usage from rust_transformer.py
- Replace build_*_response() with unified build_graphql_response()
- Update 38 test files to use new API
- Simplify RustTransformer (no schema registration needed)
- Update documentation and migration guide

Performance: 2.5x speedup in JSON transformation

Closes #XXX
```

**🎉 Phase 4 Complete! Migration finished!**

---

## 📊 Success Metrics

After completing all phases, verify these metrics:

### Functionality
- ✅ All 38+ test files pass
- ✅ Integration tests pass
- ✅ System tests pass
- ✅ Zero deprecated API usage

### Performance
- ✅ Transformation speed: ≥2.5x faster than v0.1.0
- ✅ Memory usage: Same or better
- ✅ Test execution time: Same or faster

### Code Quality
- ✅ Ruff: 0 errors
- ✅ Mypy: 0 type errors
- ✅ Test coverage: ≥90%
- ✅ Code reduced by ~100 lines (schema mapping removed)

### Documentation
- ✅ Migration guide complete
- ✅ CHANGELOG updated
- ✅ API docs updated
- ✅ README reflects v0.2.0

---

## 🆘 Troubleshooting

### Issue: Rust test binaries have linking errors

**Symptom:**
```bash
cargo build --all-targets
# error: undefined reference to `PyGILState_Release`
# error: could not compile `fraiseql_rs` (test "test_performance")
```

**Cause:** Standalone Rust test binaries can't link against Python (this is expected)

**Fix:**
This is **NOT an issue** - these are standalone Rust test binaries that aren't used in production. The actual Python extension module works perfectly. To build only what matters:

```bash
# Build the Python extension (what actually matters)
cargo build --lib              # Dev build
cargo build --release --lib    # Release build

# Verify Python module works
uv run python -c "import fraiseql_rs; print(fraiseql_rs.__version__)"
```

**Note:** The failing tests (`test_performance`, `test_new_impl`, `memory` benchmark) were comparison tests for v0.1 vs v0.2. Since v0.1 API is removed, these files have been updated to only test v0.2, but they still have PyO3 linking issues as standalone binaries. They can be safely ignored.

---

### Issue: Import Error - `SchemaRegistry` not found

**Symptom:**
```python
AttributeError: module 'fraiseql_rs' has no attribute 'SchemaRegistry'
```

**Cause:** Code still using deprecated API

**Fix:**
```bash
# Find all usages
grep -r "SchemaRegistry" src/ tests/

# Update to new API (see Phase 1)
```

### Issue: Tests failing after migration

**Symptom:**
```
AssertionError: '__typename' not found in result
```

**Cause:** Wrong field name or type name

**Fix:**
```python
# Ensure type_name is provided
result = fraiseql_rs.build_graphql_response(
    json_strings=[json_str],
    field_name="data",
    type_name="User",  # ← Must be present for __typename
    field_paths=None
)
```

### Issue: Performance regression

**Symptom:** Tests slower than v0.1.0

**Cause:** Incorrect usage of new API

**Fix:**
```python
# ❌ Bad: Decoding bytes unnecessarily
result = fraiseql_rs.build_graphql_response(...).decode('utf-8')

# ✅ Good: Keep as bytes until HTTP response
result_bytes = fraiseql_rs.build_graphql_response(...)
return RustResponseBytes(result_bytes)
```

### Issue: Null handling broken

**Symptom:** Null responses not working

**Cause:** Empty list might not represent null

**Fix:**
```python
# Check if null handling is needed in Rust API
# May need to manually build null response:
if row is None:
    null_response = f'{{"data":{{"{ field_name}":null}}}}'
    return RustResponseBytes(null_response.encode('utf-8'))
```

---

## 📚 Additional Resources

- **fraiseql_rs v0.2.0 README:** `/fraiseql_rs/README.md`
- **Rust API Documentation:** `/fraiseql_rs/API.md`
- **Performance Benchmarks:** `/fraiseql_rs/PHASE_6_BASELINE_RESULTS.md`
- **Original Migration Cleanup:** This document

---

## ✅ Final Deliverables

### Already Completed ✅
1. **Rust Implementation** - fraiseql_rs v0.2.0 fully functional
2. **Clean Rust Build** - No warnings, all deprecated APIs removed
3. **Rust Tests/Benchmarks** - Updated to v0.2 API only

### Upon Python Migration Completion
1. **Updated Python codebase** - All Python files using fraiseql_rs v0.2.0 API
2. **Passing tests** - 100% Python test success rate
3. **Documentation** - Updated README, CHANGELOG, and API docs
4. **Performance validation** - Benchmark showing ≥2.5x improvement
5. **Clean git history** - Clear commit messages documenting changes

**Estimated effort remaining:** 1-2 days for a junior engineer following this guide (Rust work already done).

**Questions?** Refer to the troubleshooting section or consult the senior engineer for guidance on:
- Complex schema transformation scenarios
- Performance optimization
- Edge cases not covered in this guide

---

*Document Version: 1.1*
*Last Updated: 2025-10-17*
*Author: FraiseQL Team*
*Status: Rust Migration Complete ✓ | Python Migration Pending*
