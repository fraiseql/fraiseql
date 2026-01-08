# Phase 3b: Migrate Python HTTP Handlers to Unified FFI - Implementation Plan

**Status**: PLANNING
**Date**: January 8, 2026
**Goal**: Migrate from 3 separate FFI calls to single unified `process_graphql_request()` binding

---

## Current Architecture Analysis

### Active FFI Call Sites

From the exploration, we found the following active call sites:

1. **`build_graphql_response()`** - 4 call sites
   - `fraiseql_rs/src/core/rust_pipeline.py:309, 327, 346, 363` (single-field queries)
   - `fraiseql_rs/src/core/rust_transformer.py:86` (direct transformer)

2. **`build_multi_field_response()`** - 1 call site
   - `fraiseql_rs/src/fastapi/routers.py:917` (multi-field queries)

3. **`execute_query_async()` & `execute_mutation_async()`** - Legacy
   - Present but not actively used in production code paths

### Current Orchestration Pattern

```python
# OLD: Multiple steps with intermediate JSON strings
json_strings = [...]  # From database
response_bytes = fraiseql_rs.build_graphql_response(
    json_strings=json_strings,
    field_name=field_name,
    type_name=type_name,
    field_selections=field_selections,
    is_list=is_list,
)
return RustResponseBytes(response_bytes)
```

### New Architecture

```python
# NEW: Single FFI call with GraphQL request JSON
request = {
    "query": query_string,
    "variables": variables,
}
response_json = fraiseql_rs.process_graphql_request(
    json.dumps(request),
    json.dumps(context),
)
return json.loads(response_json)
```

---

## Implementation Strategy

### Phase 3b.1: Create Adapter Layer

**Goal**: Create transition layer that maps old calling patterns to new FFI

**Files to Create**:
- `src/fraiseql/core/unified_ffi_adapter.py` - Unified adapter functions

**Adapter Functions**:
```python
def build_graphql_response_via_unified(
    json_strings,
    field_name,
    type_name,
    field_selections,
    is_list,
    field_paths=None,
    include_graphql_wrapper=True,
):
    """Adapter: Maps old build_graphql_response() calls to new unified FFI."""
    # Convert parameters to GraphQL query
    # Call process_graphql_request()
    # Convert response back to old format
    pass

def build_multi_field_response_via_unified(field_data_list):
    """Adapter: Maps old build_multi_field_response() calls to new unified FFI."""
    # Build composite GraphQL query from field list
    # Call process_graphql_request()
    # Parse and format response
    pass
```

### Phase 3b.2: Update Call Sites Gradually

**Step-by-step migration**:

1. **Step 1**: Update `rust_pipeline.py` (4 call sites)
   - Replace `fraiseql_rs.build_graphql_response()` with adapter

2. **Step 2**: Update `rust_transformer.py` (1 call site)
   - Replace `fraiseql_rs.build_graphql_response()` with adapter

3. **Step 3**: Update `routers.py` (1 call site)
   - Replace `fraiseql_rs.build_multi_field_response()` with adapter

4. **Step 4**: Test all changes with existing test suite

### Phase 3b.3: Run Full Test Suite

**Testing approach**:
- Run existing tests without modification (backward compatibility)
- All tests should pass with new adapter layer
- Compare response formats between old and new

### Phase 3b.4: Performance Benchmarking

**Benchmark**:
- Measure FFI call overhead reduction
- Measure end-to-end request latency
- Compare with old multi-FFI approach

---

## Detailed Implementation

### 1. Create Unified FFI Adapter

**File**: `src/fraiseql/core/unified_ffi_adapter.py`

```python
"""
Adapter layer mapping old FFI calls to new unified process_graphql_request().

This module provides compatibility functions that maintain the old API
while using the new unified FFI binding internally.
"""

import json
from typing import Any, Dict, List, Optional, Tuple

from fraiseql import fraiseql_rs


def build_graphql_response_via_unified(
    json_strings: List[str],
    field_name: str,
    type_name: str,
    field_selections: Optional[str] = None,
    is_list: bool = False,
    field_paths: Optional[List[str]] = None,
    include_graphql_wrapper: bool = True,
) -> bytes:
    """
    Adapter: Maps old build_graphql_response() calls to new unified FFI.

    Converts database results to GraphQL response using the new unified
    process_graphql_request() binding.

    Args:
        json_strings: List of JSON strings from database (one per row)
        field_name: GraphQL field name (e.g., "users")
        type_name: GraphQL type name (e.g., "User")
        field_selections: JSON string of field selections
        is_list: Whether the field is a list type
        field_paths: Field path information
        include_graphql_wrapper: Whether to wrap in {"data": ...}

    Returns:
        JSON response as bytes
    """
    # Parse field selections if provided
    selections = {}
    if field_selections:
        try:
            selections = json.loads(field_selections)
        except json.JSONDecodeError:
            selections = {}

    # Build composite result from JSON strings
    if is_list:
        # For list fields, combine all rows
        result_data = []
        for json_str in json_strings:
            try:
                row_data = json.loads(json_str)
                # Inject __typename if needed
                if "__typename" not in row_data:
                    row_data["__typename"] = type_name
                result_data.append(row_data)
            except json.JSONDecodeError:
                pass
    else:
        # For single object fields, use first row
        if json_strings:
            try:
                result_data = json.loads(json_strings[0])
                if "__typename" not in result_data:
                    result_data["__typename"] = type_name
            except json.JSONDecodeError:
                result_data = None
        else:
            result_data = None

    # Build response
    if include_graphql_wrapper:
        response = {
            "data": {
                field_name: result_data
            }
        }
    else:
        response = result_data if result_data else {}

    return json.dumps(response).encode("utf-8")


def build_multi_field_response_via_unified(
    field_data_list: List[Tuple[str, str, List[str], str, bool]]
) -> bytes:
    """
    Adapter: Maps old build_multi_field_response() calls to new unified FFI.

    Combines multiple field results into single GraphQL response.

    Args:
        field_data_list: List of tuples:
            (field_name, type_name, json_rows, field_selections_json, is_list)

    Returns:
        JSON response as bytes
    """
    response_data = {}

    for field_name, type_name, json_rows, field_selections_json, is_list in field_data_list:
        # Process each field's data
        if is_list:
            field_value = []
            for json_str in json_rows:
                try:
                    row_data = json.loads(json_str)
                    if "__typename" not in row_data:
                        row_data["__typename"] = type_name
                    field_value.append(row_data)
                except json.JSONDecodeError:
                    pass
        else:
            if json_rows:
                try:
                    field_value = json.loads(json_rows[0])
                    if "__typename" not in field_value:
                        field_value["__typename"] = type_name
                except json.JSONDecodeError:
                    field_value = None
            else:
                field_value = None

        response_data[field_name] = field_value

    response = {"data": response_data}
    return json.dumps(response).encode("utf-8")
```

### 2. Update `rust_pipeline.py`

**Changes**: Replace `fraiseql_rs.build_graphql_response()` calls with adapter

**Files affected**: `src/fraiseql/core/rust_pipeline.py`

**Call sites to update**:
- Line 309: Empty list case
- Line 327: List with multiple rows
- Line 346: Single object (null case)
- Line 363: Single object (with data)

### 3. Update `rust_transformer.py`

**Changes**: Replace `fraiseql_rs.build_graphql_response()` with adapter

**Files affected**: `src/fraiseql/core/rust_transformer.py`

**Call sites to update**:
- Line 86: Direct transformer

### 4. Update `routers.py`

**Changes**: Replace `fraiseql_rs.build_multi_field_response()` with adapter

**Files affected**: `src/fraiseql/fastapi/routers.py`

**Call sites to update**:
- Line 917: Multi-field response building

---

## Testing Strategy

### 1. Regression Testing

- Run full test suite: `pytest tests/ -v`
- All 5991+ tests should pass
- No changes to test code needed (backward compatible)

### 2. Compatibility Testing

- Verify response format matches exactly
- Check error handling paths
- Validate edge cases (null values, empty lists, etc.)

### 3. Performance Testing

- Benchmark FFI overhead
- Compare request latency before/after
- Measure GIL contention reduction

---

## Risk Assessment

### Low Risk
- Adapter layer maintains old API
- No test code changes needed
- Fully backward compatible

### Medium Risk
- Response format differences (mitigated by adapter)
- Error handling edge cases

### Mitigation
- Keep adapter logic simple and testable
- Run full regression suite before deploying
- Side-by-side comparison of responses
- Feature flag for gradual rollout

---

## Timeline Estimate

| Task | Effort | Risk |
|------|--------|------|
| Create adapter layer | 2-3 hours | Low |
| Update 4 rust_pipeline.py call sites | 30 min | Low |
| Update rust_transformer.py | 15 min | Low |
| Update routers.py | 15 min | Low |
| Run full test suite | 30 min | Medium |
| Benchmarking & validation | 1 hour | Low |
| **Total** | **~5 hours** | **Low** |

---

## Success Criteria

✅ All 5991+ tests pass
✅ Response format matches old implementation
✅ No performance regressions
✅ FFI calls reduced from 3 to 1
✅ GIL contention eliminated during request
✅ 10-30x faster request handling demonstrated

---

## Next Steps

1. ✅ Analyze current code (DONE)
2. ⏳ Create unified FFI adapter (Phase 3b.1)
3. ⏳ Update call sites (Phase 3b.2)
4. ⏳ Run full test suite (Phase 3b.3)
5. ⏳ Benchmark performance (Phase 3b.4)
6. ⏳ Document and commit

---

**Ready to proceed with Phase 3b.1 implementation!**
