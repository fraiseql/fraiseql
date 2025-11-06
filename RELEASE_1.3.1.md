# FraiseQL 1.3.1 Release Notes

**Release Date**: January 6, 2025
**Type**: Bug Fix Release

## Overview

This patch release fixes a critical bug in the Rust pipeline where `db.find()` incorrectly returned a single dict object instead of a list when exactly one record matched the query filter.

## 🐛 Bug Fix

### Issue #114: db.find() returns dict instead of list for single record

**Problem**:
When using `db.find()` with a `where` filter that matched exactly one record, the method returned a single dict object `{...}` instead of a list containing one dict `[{...}]`.

**Impact**:
- Broke GraphQL queries expecting `list[T]` return type
- Tests failed with type errors when accessing single-record results
- Violated GraphQL type contract for list fields

**Root Cause**:
The Rust response builder (`fraiseql_rs/src/pipeline/builder.rs`) was checking `json_rows.len() == 1` to decide whether to wrap results in an array, rather than respecting the query's intent (list vs. single object query).

**Solution**:
- Added `is_list: Option<bool>` parameter to the Rust FFI layer
- Updated both response builders to respect the `is_list` parameter:
  - `build_with_schema()` - for schema-aware transformations
  - `build_zero_copy()` - for zero-copy transformations
- Python layer now explicitly passes:
  - `is_list=True` for `db.find()` calls
  - `is_list=False` for `db.find_one()` calls

**Behavior After Fix**:
```python
# db.find() - always returns list
await db.find("routers")           # [] or [{...}] or [{...}, {...}]
await db.find("routers", limit=1)  # [{...}]  ✅ Fixed!

# db.find_one() - always returns single object
await db.find_one("router", id=1)  # {...}
```

## 📦 Changes

### Modified Files

**Rust (`fraiseql_rs`)**:
- `src/lib.rs` - Added `is_list` parameter to FFI function
- `src/pipeline/builder.rs` - Updated response builders
  - Renamed `build_legacy_response` → `build_zero_copy` for clarity
  - Renamed `build_with_schema_awareness` → `build_with_schema`
  - Both now use `is_list.unwrap_or(true)` instead of row count

**Python**:
- `src/fraiseql/core/rust_pipeline.py` - Passes `is_list` parameter based on query type

**Tests**:
- `tests/regression/test_issue_114_single_record_list.py` - New comprehensive test suite (4 tests)

**Documentation**:
- `CHANGELOG.md` - Added 1.3.1 release notes
- `RELEASE_1.3.1.md` - This release document

### Version Updates
- `pyproject.toml`: `1.3.0` → `1.3.1`
- `fraiseql_rs/Cargo.toml`: `1.3.0` → `1.3.1`
- `src/fraiseql/__init__.py`: `__version__ = "1.3.1"`

## ✅ Testing

### New Tests
Added comprehensive regression test suite covering:
- ✅ Single record with `is_list=True` returns array
- ✅ Single record with `is_list=False` returns object
- ✅ Multiple records with `is_list=True` returns array
- ✅ Zero records with `is_list=True` returns empty array

### Test Results
```
tests/regression/test_issue_114_single_record_list.py::
  test_rust_pipeline_single_record_with_is_list_true ........... PASSED
  test_rust_pipeline_single_record_with_is_list_false .......... PASSED
  test_rust_pipeline_multiple_records_with_is_list_true ........ PASSED
  test_rust_pipeline_zero_records_with_is_list_true ............ PASSED

4 passed in 0.04s
```

### Integration Tests
All existing integration tests pass:
- ✅ 45/45 caching integration tests
- ✅ All repository tests
- ✅ All pipeline tests

## 🔄 Migration

### For Users

**No action required!** This is a bug fix that restores correct behavior.

If you implemented workarounds for this bug, you should remove them:

```python
# Before (workaround)
result = await db.find("routers", where={"id": {"eq": router_id}})
if isinstance(result, dict):
    result = [result]  # ❌ Remove this workaround

# After (correct)
result = await db.find("routers", where={"id": {"eq": router_id}})
# result is always a list ✅
```

### Backward Compatibility

✅ **100% backward compatible**
- No breaking changes
- No API changes
- Fixes incorrect behavior to match documented contract

## 📚 References

- **Issue**: https://github.com/fraiseql/fraiseql/issues/114
- **Test Suite**: `tests/regression/test_issue_114_single_record_list.py`
- **Changelog**: `CHANGELOG.md` (lines 8-37)

## 🏗️ Technical Details

### Code Changes

**Rust FFI Layer**:
```rust
// Before
pub fn build_graphql_response(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
    field_selections: Option<String>,
) -> PyResult<Vec<u8>>

// After
pub fn build_graphql_response(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
    field_selections: Option<String>,
    is_list: Option<bool>,  // ✨ New parameter
) -> PyResult<Vec<u8>>
```

**Response Builder Logic**:
```rust
// Before (incorrect)
if json_rows.len() == 1 {
    // Return single object
} else {
    // Return array
}

// After (correct)
if is_list.unwrap_or(true) {
    // Return array
} else {
    // Return single object
}
```

### Performance Impact

**None** - The fix only adds a single boolean parameter check:
- No additional allocations
- No additional parsing
- Same zero-copy pipeline
- Same transformation performance

## 📞 Support

For questions or issues:
- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Documentation: https://docs.fraiseql.com

---

**Full Changelog**: https://github.com/fraiseql/fraiseql/blob/main/CHANGELOG.md
