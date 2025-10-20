# Rust Interface Fix Needed

## Problem Summary
After completing Phase 1 of the unified Rust architecture cleanup, integration tests are failing with:
```
AttributeError: module 'fraiseql_rs' has no attribute 'build_list_response'
```

## What We Did
- ✅ Deleted deprecated Python modules (passthrough_mixin, raw_json_executor, etc.)
- ✅ Refactored `db.py` to remove mode detection and simplify to Rust-only execution
- ✅ Updated response handlers, routers, and other components
- ✅ Made `field_name` parameter optional in `find()`/`find_one()` methods
- ✅ Fixed WHERE clause processing for dict filters

## Current Error
When running repository integration tests, the code reaches:
```python
# In src/fraiseql/core/rust_pipeline.py
response_bytes = fraiseql_rs.build_list_response(
    field_name, type_name, json_rows, field_paths
)
```

But `fraiseql_rs.build_list_response` does not exist.

## Investigation Needed
1. **Check Rust module interface**: What functions are actually exported by `fraiseql_rs`?
   ```bash
   python -c "import fraiseql_rs; print(dir(fraiseql_rs))"
   ```

2. **Find correct function name**: The function might be named differently, e.g.:
   - `build_response` (without "list")
   - `create_list_response`
   - `transform_list`
   - Something else

3. **Check function signature**: Once found, verify the parameters match what's being passed.

4. **Update the call**: Fix `src/fraiseql/core/rust_pipeline.py` to use the correct function name and signature.

## Failing Test
```
tests/integration/database/repository/test_dict_where_mixed_filters_bug.py::TestDictWhereMixedFiltersBug::test_dict_where_with_nested_filter_only
```

## Context
- The test calls `repo.find("test_router_config_view", where=where_dict)`
- This goes through the unified Rust pipeline
- The Rust pipeline tries to call `fraiseql_rs.build_list_response()` but fails
- The WHERE clause processing is working (no more psycopg errors)
- The issue is specifically in the Rust function call

## Next Steps
1. Investigate the actual `fraiseql_rs` module interface
2. Find the correct function name for building list responses
3. Update the Python code to call the correct Rust function
4. Verify all tests pass

## Files to Check
- `src/fraiseql/core/rust_pipeline.py` - Contains the failing function call
- `fraiseql_rs/src/lib.rs` - Rust module exports
- `fraiseql_rs/src/response.rs` - Likely contains response building functions</content>
</xai:function_call
