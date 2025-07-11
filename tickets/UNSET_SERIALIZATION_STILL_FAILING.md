# UNSET Serialization Still Failing in PrintOptim Backend

**Date:** 2025-07-11  
**Reporter:** PrintOptim Backend Team  
**FraiseQL Version:** 0.1.0b10  
**Status:** Critical - Blocking 137+ tests  

## Issue Description

Despite the UNSET serialization fix documented in `FRAISEQL_UNSET_SERIALIZATION_FIX.md` being included in version 0.1.0b10, PrintOptim Backend is still experiencing "Object of type Unset is not JSON serializable" errors in GraphQL mutations.

## Error Details

```
AssertionError: createMachineItem returned None. Full response: {
    'data': {'createMachineItem': None}, 
    'errors': [{
        'message': 'Object of type Unset is not JSON serializable', 
        'locations': [{'line': 3, 'column': 13}], 
        'path': ['createMachineItem'], 
        'extensions': {}
    }]
}
```

## Affected Mutations

The error occurs across many mutations, including:
- `createMachineItem`
- `createMachine`
- `updateMachine`
- Many other mutations using optional fields with UNSET defaults

## Investigation Results

1. **PrintOptim has a custom JSON response class** at `src/printoptim_backend/entrypoints/api/custom_json_response.py`:
   ```python
   class UnsetSafeJSONEncoder(json.JSONEncoder):
       def default(self, obj: Any) -> Any:
           if obj is UNSET:
               return None
           return super().default(obj)
   ```

2. **This custom response class is NOT being used** - it was created but never integrated into the GraphQL endpoints

3. **PrintOptim uses `create_fraiseql_app()`** which should handle this internally according to the fix documentation

## Possible Root Causes

1. **The fix might not be active in `create_fraiseql_app()`** - The fix documentation shows manual endpoint configuration with `response_class=FraiseQLJSONResponse`, but `create_fraiseql_app()` might not apply this automatically

2. **The error might occur before response serialization** - Perhaps the UNSET values are being serialized during error construction, not just in the final response

3. **Different code path** - The error might be happening in a different serialization path that wasn't covered by the fix

## Reproduction Steps

1. Run PrintOptim Backend tests: `uv run pytest tests/api/test_machine_item_mutations.py::test_create_machine_item_with_order -xvs`
2. Observe the "Object of type Unset is not JSON serializable" error

## Expected Behavior

UNSET values should be automatically converted to `null` in JSON responses, including error responses, without requiring any code changes in PrintOptim Backend.

## Suggested Solutions

1. **Ensure `create_fraiseql_app()` applies the custom response class** - The fix should work transparently when using the high-level API

2. **Check all JSON serialization paths** - Not just the final response, but also:
   - Error construction
   - Debug/extension data
   - Logging output
   
3. **Provide migration guide** - If manual configuration is required, document how projects using `create_fraiseql_app()` should apply the fix

## Workaround Attempts

We tried creating a custom JSON response class but haven't integrated it because:
1. We expected FraiseQL 0.1.0b10 to handle this automatically
2. We're using `create_fraiseql_app()` which creates its own FastAPI instance

## Impact

This issue is blocking 137 test failures in PrintOptim Backend, preventing us from validating other fixes and completing the SQLAlchemy to psycopg migration.