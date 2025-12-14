# Phase: Fix Remaining CI Failures

## Current Situation Assessment

### ✅ **Successfully Fixed:**
- **Version Consistency**: All version files now match 1.8.1
- **Documentation Validation**: All 347 markdown links valid
- **Examples Compliance**: 22/22 examples fully compliant
- **Lint Issues**: Fixed major linting violations in verification scripts
- **Pre-commit Hooks**: All hooks now pass

### ❌ **Remaining Failures:**

#### 1. Unit Test: `test_rust_binding_error`
**File**: `tests/unit/mutations/test_rust_mutation_binding.py:107`
**Issue**: `assert response["data"]["createUser"]["code"] == 422` fails with `500 == 422`
**Status**: "failed:validation" returns HTTP 500 instead of expected 422

**Root Cause Analysis**:
- Test sets mutation status to `"failed:validation"`
- Expects HTTP code 422 (Unprocessable Entity for validation errors)
- Rust code in `fraiseql_rs/src/mutation/response_builder.rs::map_status_to_code()` returns 500 for "failed:*" statuses
- The logic treats "failed:validation" as a generic failure (500) instead of validation error (422)

**Code Locations**:
- Test: `tests/unit/mutations/test_rust_mutation_binding.py:82-92`
- Rust logic: `fraiseql_rs/src/mutation/response_builder.rs:478-484`

#### 2. Tox Validation Failure
**Issue**: Unknown - needs investigation
**Status**: Failing but details not fully analyzed

### 🔧 **Attempted Fixes (Unsuccessful):**

1. **Modified Rust `application_code()` method** in `fraiseql_rs/src/mutation/mod.rs`
   - Added check for "failed:validation" → 422
   - Issue: This method is not used for HTTP status codes

2. **Modified Rust `map_status_to_code()` function** in `fraiseql_rs/src/mutation/response_builder.rs`
   - Added exact match for "failed:validation" → 422
   - Added debug logging to verify execution
   - Issue: Debug logs not appearing, suggesting function not called or rebuilt properly

3. **Rust Module Import Issues**:
   - Fixed `#[pymodule]` name from `_fraiseql_rs` to `fraiseql_rs`
   - Resolved import errors
   - Extension rebuilds successfully

### 🎯 **Next Steps Required:**

#### Immediate Priority:
1. **Fix `test_rust_binding_error`**:
   - Determine why `map_status_to_code()` returns 500 instead of 422 for "failed:validation"
   - Verify Rust extension rebuild includes changes
   - Check if debug logging is working

2. **Investigate Tox Validation**:
   - Run tox locally to see specific failures
   - Check tox configuration and test matrix

#### Technical Questions:
- Why doesn't the Rust code change take effect despite successful rebuild?
- Is there caching or multiple code paths for status code mapping?
- Should the test use "validation:invalid_email" instead of "failed:validation"?

### 📋 **Action Items for Next Agent:**

1. **Debug Rust Status Code Mapping**:
   ```bash
   # Verify debug output appears
   cd /home/lionel/code/fraiseql
   python -m pytest tests/unit/mutations/test_rust_mutation_binding.py::test_rust_binding_error -v -s
   ```

2. **Check Rust Extension Rebuild**:
   ```bash
   # Ensure changes are actually applied
   cd fraiseql_rs
   maturin develop --release --force
   ```

3. **Investigate Tox Issues**:
   ```bash
   # Run tox to see specific failures
   tox -v
   ```

4. **Alternative Approach**: Update test expectation if "failed:validation" should legitimately return 500

### 🔍 **Key Files to Examine:**
- `tests/unit/mutations/test_rust_mutation_binding.py` (test case)
- `fraiseql_rs/src/mutation/response_builder.rs` (status code mapping)
- `fraiseql_rs/src/mutation/mod.rs` (alternative status code mapping)
- `tox.ini` (tox configuration)

### 💡 **Hypothesis:**
The issue may be that the Python extension is not properly loading the updated Rust code, or there are multiple code paths for status code determination. The debug logging should confirm if `map_status_to_code` is being called with the expected parameters.
