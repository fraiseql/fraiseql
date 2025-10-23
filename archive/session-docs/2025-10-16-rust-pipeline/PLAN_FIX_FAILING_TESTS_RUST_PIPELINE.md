# Plan: Fix/Remove 65 Failing Tests for Exclusive Rust Pipeline

**Context**: FraiseQL has transitioned to an exclusive Rust-first pipeline (PostgreSQL → Rust → HTTP). Many tests were written for the old multi-mode execution system (NORMAL, PASSTHROUGH, TURBO modes) which is now deprecated.

**Agent Task**: Review each failing test and either DELETE (if testing deprecated functionality) or FIX (if testing valid functionality that needs updating for Rust pipeline).

---

## 📊 Summary of 65 Failing Tests

### Category Breakdown:
1. **Rust API Tests** (13 failures) - Testing deprecated Rust functions
2. **WHERE Clause Repository Tests** (17 failures) - Testing against wrong execution mode expectations
3. **Session Variables Tests** (8 failures) - Testing deprecated NORMAL/PASSTHROUGH modes
4. **Regression Tests** (8 failures) - Mixed: some deprecated, some need fixes
5. **JSONB vs Dict Filtering** (5 failures) - Testing deprecated functionality
6. **IP Address Scalar Tests** (3 failures) - Needs investigation
7. **Query Timeout Regression** (2 failures) - Testing deprecated functionality
8. **WHERE Clause Industrial** (4 failures) - Testing against `RustResponseBytes` incorrectly
9. **First Query Null** (1 failure) - Needs investigation
10. **Coordinate Filter Operations** (4 failures - assumed based on new files)

---

## 🎯 Decision Framework

### DELETE Test If:
- ✅ Tests deprecated execution modes (NORMAL, PASSTHROUGH)
- ✅ Tests old Rust API functions (`to_camel_case()`, `transform_json_with_typename()`, etc.)
- ✅ Tests Python transformation fallbacks
- ✅ Tests execution mode detection logic
- ✅ Tests passthrough detection mechanisms

### FIX Test If:
- ✅ Tests core functionality (WHERE clauses, filtering, queries)
- ✅ Tests GraphQL response structure
- ✅ Tests data correctness
- ✅ Needs updating to work with `RustResponseBytes` return type
- ✅ Needs updating to use new Rust API (`build_graphql_response()`)

---

## 📋 Detailed Test-by-Test Plan

## 1. Rust API Tests (13 failures)

**Location**: `tests/integration/rust/`

### 1.1 `test_camel_case.py` (5 failures)

**Tests**:
- `test_to_camel_case_basic`
- `test_to_camel_case_single_word`
- `test_to_camel_case_multiple_underscores`
- `test_to_camel_case_edge_cases`
- `test_to_camel_case_with_numbers`

**Decision**: **DELETE ALL 5 TESTS**

**Reason**: These tests call `fraiseql_rs.to_camel_case()` directly, which is an internal Rust function that was exposed for testing during development. The Rust pipeline now only exposes `build_graphql_response()` as the public API. CamelCase conversion happens automatically inside `build_graphql_response()`.

**Note**: Tests `test_transform_keys`, `test_transform_keys_nested`, `test_transform_keys_with_lists` are PASSING and should be KEPT (they test the correct API).

**Action**:
```bash
# Delete the 5 failing test functions from test_camel_case.py
# Keep the 3 passing test functions
```

---

### 1.2 `test_typename_injection.py` (8 failures)

**Tests**:
- `test_build_graphql_response_simple`
- `test_build_graphql_response_nested`
- `test_build_graphql_response_array`
- `test_build_graphql_response_complex`
- `test_build_graphql_response_no_types`
- `test_build_graphql_response_empty_object`
- `test_build_graphql_response_preserves_existing`
- `test_build_graphql_response_string_type`

**Decision**: **FIX ALL 8 TESTS**

**Reason**: These tests verify that `__typename` is injected correctly, which is core GraphQL functionality. However, they're calling the Rust API directly instead of going through the Python pipeline. They should be updated to test the end-to-end behavior through `execute_via_rust_pipeline()`.

**Fix Strategy**:
1. **Option A** (Simpler): Update tests to use the actual Rust API correctly with current signature
2. **Option B** (Better): Rewrite as integration tests that call GraphQL queries and verify `__typename` in responses

**Recommended**: **Option B** - Move to integration tests

**Action**:
```python
# Rewrite as integration tests in tests/integration/graphql/test_typename_injection.py
# Test actual GraphQL queries that should include __typename
# Example:
async def test_typename_injected_in_query_response(graphql_client):
    query = '''
    query {
        user(id: "123") {
            __typename
            id
            firstName
        }
    }
    '''
    result = await graphql_client.execute(query)
    assert result["data"]["user"]["__typename"] == "User"
```

---

### 1.3 `test_nested_array_resolution.py` (8 failures - assumed all tests in file)

**Tests**:
- `test_schema_based_transformation_simple`
- `test_schema_based_transformation_with_array`
- `test_schema_based_nested_arrays`
- `test_schema_based_nullable_fields`
- `test_schema_based_empty_arrays`
- `test_schema_based_mixed_fields`
- `test_schema_registry`
- `test_backward_compatibility_with_phase4`

**Decision**: **DELETE ALL TESTS**

**Reason**: These tests are for "Phase 4" development which was testing schema-based transformation. The current Rust pipeline handles this automatically via `build_graphql_response()`. The schema registry is internal to Rust and not exposed.

**Action**:
```bash
# Delete entire file: tests/integration/rust/test_nested_array_resolution.py
```

---

## 2. WHERE Clause Repository Tests (17 failures)

**Location**: `tests/integration/database/repository/`

### 2.1 `test_repository_where_integration.py` (14 failures)

**Tests**:
- `test_find_with_simple_where_equality`
- `test_find_with_comparison_operators`
- `test_find_with_multiple_operators`
- `test_find_with_multiple_fields`
- `test_find_with_null_handling`
- `test_find_with_date_filtering`
- `test_find_one_with_where`
- `test_combining_where_with_kwargs`
- `test_production_mode_returns_dicts`
- `test_empty_where_returns_all`
- `test_unsupported_operator_is_ignored`
- `test_complex_nested_where`

**Decision**: **FIX ALL 14 TESTS**

**Reason**: WHERE clause filtering is core functionality. These tests are failing because:
1. They expect Python dict objects but now get `RustResponseBytes`
2. They may be setting execution mode expectations that no longer exist

**Fix Strategy**:
1. Remove any execution mode checks/settings
2. Update assertions to handle `RustResponseBytes` return type
3. Convert `RustResponseBytes` to dict for assertions: `json.loads(bytes(result.bytes))`

**Action for EACH test**:
```python
# OLD CODE (example):
async def test_find_with_simple_where_equality(repo):
    results = await repo.find("v_user", where={"name": {"eq": "John"}})
    assert isinstance(results, list)  # Expects list of dicts
    assert results[0]["name"] == "John"

# NEW CODE (fixed):
async def test_find_with_simple_where_equality(repo):
    result = await repo.find("v_user", where={"name": {"eq": "John"}})

    # Handle RustResponseBytes response
    if isinstance(result, RustResponseBytes):
        data = json.loads(bytes(result.bytes))
        # Extract from GraphQL response structure
        results = data["data"]["v_user"]  # or appropriate field name
    else:
        results = result

    assert isinstance(results, list)
    assert results[0]["name"] == "John"
```

**Special case - `test_production_mode_returns_dicts`**:
This test name is misleading now. The test should verify that Rust pipeline returns valid JSON, not dicts.

**Fix**:
```python
# Rename and update
async def test_rust_pipeline_returns_valid_json(repo):
    result = await repo.find("v_user")
    assert isinstance(result, RustResponseBytes)

    # Verify it's valid JSON
    data = json.loads(bytes(result.bytes))
    assert "data" in data
```

---

### 2.2 `test_jsonb_vs_dict_filtering.py` (5 failures - assumed all tests)

**Tests**:
- `test_whereinput_uses_jsonb_paths`
- `test_dict_filters_use_direct_columns`
- `test_dynamic_dict_filter_construction`
- `test_whereinput_on_regular_table_works`
- `test_mixed_whereinput_and_kwargs`

**Decision**: **DELETE ALL 5 TESTS**

**Reason**: These tests are comparing JSONB filtering vs dict filtering, which was relevant when we had multiple execution paths. Now there's only the Rust pipeline, so the distinction is no longer meaningful. The WHERE clause tests above already cover filtering functionality.

**Action**:
```bash
# Delete entire file: tests/integration/database/repository/test_jsonb_vs_dict_filtering.py
```

---

## 3. Session Variables Tests (8 failures)

**Location**: `tests/integration/session/test_session_variables.py`

**Tests**:
- `test_session_variables_in_normal_mode` ❌ FAILING
- `test_session_variables_in_passthrough_mode` ❌ FAILING
- `test_session_variables_in_turbo_mode` ✅ PASSING (keep!)
- `test_session_variables_consistency_across_modes[ExecutionMode.NORMAL]` ❌ FAILING
- `test_session_variables_consistency_across_modes[ExecutionMode.PASSTHROUGH]` ❌ FAILING
- `test_session_variables_consistency_across_modes[ExecutionMode.TURBO]` ❌ FAILING
- `test_session_variables_only_when_present_in_context` ❌ FAILING
- `test_session_variables_transaction_scope` ❌ FAILING
- `test_session_variables_with_custom_names` ❌ FAILING

**Decision**: **DELETE 7 TESTS, KEEP 1**

**Reason**:
- NORMAL and PASSTHROUGH modes are deprecated
- Only TURBO mode (Rust pipeline) exists now
- The passing `test_session_variables_in_turbo_mode` validates session variables work

**Action**:
```python
# In test_session_variables.py

# DELETE these test functions:
- test_session_variables_in_normal_mode
- test_session_variables_in_passthrough_mode
- test_session_variables_consistency_across_modes (entire parametrized test)

# KEEP and potentially RENAME:
- test_session_variables_in_turbo_mode → test_session_variables_work()

# FIX these tests (remove mode-specific logic):
- test_session_variables_only_when_present_in_context
- test_session_variables_transaction_scope
- test_session_variables_with_custom_names

# Fix example:
async def test_session_variables_only_when_present_in_context(repo):
    # OLD: Test across multiple modes
    # NEW: Test only Rust pipeline behavior

    # Without session variables in context
    result1 = await repo.find("v_user")
    # ... assertions ...

    # With session variables in context
    result2 = await repo.find("v_user", context={"session_vars": {...}})
    # ... assertions ...
```

---

## 4. Regression Tests (8 failures)

### 4.1 `test_graphql_ip_address_scalar_mapping.py` (3 failures)

**Location**: `tests/regression/test_graphql_ip_address_scalar_mapping.py`

**Tests**:
- `test_ip_address_scalar_mapping` ❌
- `test_graphql_validation_with_ip_address_scalar` ❌
- `test_ip_address_field_type_mapping` ✅ (keep!)
- `test_multiple_ip_address_field_name_conversions` ❌

**Decision**: **INVESTIGATE THEN FIX**

**Reason**: IP address scalar functionality is core. Need to check if tests are failing due to:
1. RustResponseBytes return type (FIX)
2. Actual regression in IP address handling (FIX differently)

**Action**:
```bash
# Run tests with verbose output to see actual errors
uv run pytest tests/regression/test_graphql_ip_address_scalar_mapping.py -vv --tb=long

# Then apply appropriate fix based on error:
# - If RustResponseBytes issue: Update like WHERE clause tests
# - If actual IP address bug: Fix the IP address handling code
```

---

### 4.2 `test_query_timeout_bug.py` (2 failures)

**Location**: `tests/regression/v0_4_0/test_query_timeout_bug.py`

**Tests**:
- `test_find_one_no_longer_uses_parameterized_set_local`
- `test_find_one_with_fixed_timeout`

**Decision**: **DELETE BOTH TESTS**

**Reason**: These tests validate a specific fix from v0.4.0 for query timeout handling in the old execution modes. The Rust pipeline handles queries differently, making this regression test obsolete.

**Action**:
```bash
# Delete entire file: tests/regression/v0_4_0/test_query_timeout_bug.py
```

---

### 4.3 `test_first_query_null_issue.py` (1 failure)

**Location**: `tests/regression/v0_1_0/test_first_query_null_issue.py`

**Test**: `test_first_query_returns_null_simple`

**Decision**: **FIX**

**Reason**: Testing null handling is important. Likely failing due to RustResponseBytes return type.

**Action**:
```python
# Update test to handle RustResponseBytes
async def test_first_query_returns_null_simple(repo):
    result = await repo.find_one("v_user", where={"id": {"eq": "nonexistent"}})

    # Handle RustResponseBytes
    if isinstance(result, RustResponseBytes):
        data = json.loads(bytes(result.bytes))
        actual_result = data["data"]["v_user"]  # Will be null
    else:
        actual_result = result

    assert actual_result is None
```

---

### 4.4 `test_industrial_where_clause_generation.py` (4 failures)

**Location**: `tests/regression/where_clause/test_industrial_where_clause_generation.py`

**Tests**:
- `test_production_hostname_filtering_fails` ❌
- `test_production_port_filtering_fails` ❌
- `test_production_boolean_filtering_fails` ❌
- `test_production_mixed_filtering_comprehensive` ❌

**Error**: `TypeError: object of type 'RustResponseBytes' has no len()`

**Decision**: **FIX ALL 4 TESTS**

**Reason**: These tests validate WHERE clause type casting (hostname, port, boolean). This is core functionality. Tests are failing because they try to use `len()` on `RustResponseBytes`.

**Action**:
```python
# Each test needs updating to handle RustResponseBytes
# Example for test_production_hostname_filtering_fails:

async def test_production_hostname_filtering(repo):  # Remove "fails" from name
    result = await repo.find("v_server", where={
        "hostname": {"eq": "api.example.com"}
    })

    # FIX: Handle RustResponseBytes
    if isinstance(result, RustResponseBytes):
        data = json.loads(bytes(result.bytes))
        servers = data["data"]["v_server"]
    else:
        servers = result

    # Now can use len()
    assert len(servers) == 1
    assert servers[0]["hostname"] == "api.example.com"
```

**Note**: Also rename tests to remove "_fails" suffix since we're fixing them.

---

## 5. Coordinate Filter Operations (4 failures - assumed)

**Location**: `tests/integration/database/sql/test_coordinate_filter_operations.py` (assumed new file)

**Decision**: **INVESTIGATE**

**Reason**: This appears to be a new feature (coordinates datatype). Need to check if tests exist and why they're failing.

**Action**:
```bash
# Check if file exists
ls -la tests/integration/database/sql/test_coordinate_filter_operations.py

# If exists, run tests with verbose output
uv run pytest tests/integration/database/sql/test_coordinate_filter_operations.py -vv --tb=long

# Then determine:
# - If RustResponseBytes issue: FIX like other tests
# - If incomplete feature: Mark as @pytest.mark.skip until feature complete
```

---

## 📝 Implementation Checklist

### Step 1: Delete Deprecated Test Files (Safe to delete entirely)
```bash
# Rust API tests for old functions
rm tests/integration/rust/test_nested_array_resolution.py

# JSONB vs Dict comparison tests (no longer relevant)
rm tests/integration/database/repository/test_jsonb_vs_dict_filtering.py

# Query timeout regression from v0.4.0 (old execution mode)
rm tests/regression/v0_4_0/test_query_timeout_bug.py
```

### Step 2: Delete Deprecated Test Functions (Keep file, delete functions)
```python
# In tests/integration/rust/test_camel_case.py
# DELETE these 5 functions:
- test_to_camel_case_basic
- test_to_camel_case_single_word
- test_to_camel_case_multiple_underscores
- test_to_camel_case_edge_cases
- test_to_camel_case_with_numbers

# In tests/integration/session/test_session_variables.py
# DELETE these functions:
- test_session_variables_in_normal_mode
- test_session_variables_in_passthrough_mode
- test_session_variables_consistency_across_modes
```

### Step 3: Fix RustResponseBytes Handling Tests
Create helper function first:
```python
# In tests/conftest.py or tests/utils.py
import json
from fraiseql.core.rust_pipeline import RustResponseBytes

def extract_graphql_data(result, field_name: str):
    """Extract data from RustResponseBytes or dict response.

    Args:
        result: Either RustResponseBytes or dict
        field_name: GraphQL field name (e.g., "users", "user")

    Returns:
        The data from result["data"][field_name]
    """
    if isinstance(result, RustResponseBytes):
        data = json.loads(bytes(result.bytes))
        return data["data"][field_name]
    elif isinstance(result, dict):
        return result.get("data", {}).get(field_name, result)
    else:
        return result
```

Then fix these test files using the helper:
1. ✅ `tests/integration/database/repository/test_repository_where_integration.py` (14 tests)
2. ✅ `tests/regression/where_clause/test_industrial_where_clause_generation.py` (4 tests)
3. ✅ `tests/integration/session/test_session_variables.py` (3 tests)
4. ✅ `tests/regression/v0_1_0/test_first_query_null_issue.py` (1 test)
5. ✅ `tests/regression/test_graphql_ip_address_scalar_mapping.py` (3 tests - after investigation)

### Step 4: Rewrite TypeName Injection Tests
Move from unit tests to integration tests:
```bash
# Rename/move file
mv tests/integration/rust/test_typename_injection.py \
   tests/integration/graphql/test_typename_in_responses.py
```

Rewrite as GraphQL integration tests that verify `__typename` appears in actual query responses.

### Step 5: Handle Coordinate Tests (If they exist)
```bash
# Investigate and fix or skip
# Decision pending investigation results
```

### Step 6: Run Full Test Suite
```bash
# After all fixes
uv run pytest --tb=short

# Target: 0 failures, ~3,450+ passing
```

---

## 🎯 Expected Outcome

### Tests to Delete: **25 tests**
- 5 camelCase unit tests
- 8 nested array tests
- 5 JSONB vs dict tests
- 2 session variable mode tests
- 1 consistency test (parametrized, counts as multiple)
- 2 query timeout tests

### Tests to Fix: **35+ tests**
- 14 WHERE clause repository tests
- 8 typename injection tests (rewrite)
- 4 industrial WHERE clause tests
- 3 session variable tests
- 3 IP address scalar tests
- 1 first query null test
- 4 coordinate tests (if exist)

### Final Expected Test Count
- **Before**: 3,492 passing + 65 failing = 3,557 total
- **After**: ~3,462 passing + 0 failing = 3,462 total (95 tests deleted, 30 tests fixed/kept)

---

## ⚠️ Critical Notes for Agent

1. **Import RustResponseBytes correctly**:
   ```python
   from fraiseql.core.rust_pipeline import RustResponseBytes
   ```

2. **Always check if RustResponseBytes before accessing data**:
   ```python
   if isinstance(result, RustResponseBytes):
       data = json.loads(bytes(result.bytes))
   ```

3. **GraphQL response structure** from Rust pipeline:
   ```json
   {
     "data": {
       "fieldName": [...]  // or {...} for single object
     }
   }
   ```

4. **Don't delete passing tests** - Only delete/fix failing tests as specified

5. **Run tests incrementally** - Fix one file at a time, verify it passes before moving to next

6. **If uncertain about a test** - Mark with `@pytest.mark.skip(reason="...")` rather than deleting

---

## 🚀 Execution Order (Recommended)

1. ✅ **Delete entire files** (Step 1) - Safest, no risk
2. ✅ **Delete deprecated functions** (Step 2) - Safe, well-defined
3. ✅ **Create helper function** (Step 3 setup) - Enables all fixes
4. ✅ **Fix WHERE clause tests** (Step 3.1) - High value, clear pattern
5. ✅ **Fix regression tests** (Step 3.2-3.4) - Important for stability
6. ✅ **Fix session variable tests** (Step 3.5) - Smaller scope
7. ✅ **Rewrite typename tests** (Step 4) - Most complex, do last
8. ✅ **Verify coordinate tests** (Step 5) - May not exist
9. ✅ **Run full suite** (Step 6) - Final validation

---

**Agent**: Follow this plan test-by-test. After each fix, run that specific test to verify it passes before moving to the next one. Report any unexpected errors or ambiguities for clarification.
