# FraiseQL UNSET Serialization Fix

## Summary

Fixed the JSON serialization issue where GraphQL error responses containing `UNSET` values would fail with "Object of type Unset is not JSON serializable".

## The Problem

When using `UNSET` as default values in input types:

```python
from fraiseql.types.definitions import UNSET

@fraiseql.input
class CreateMachineItemInput:
    machine_id: uuid.UUID
    source_id: uuid.UUID
    order_id: uuid.UUID | None = UNSET  # This caused serialization errors
```

If a mutation returned an error that included the input object in error details, FraiseQL would fail to serialize the GraphQL response because `UNSET` cannot be converted to JSON.

## The Solution

1. **Enhanced `FraiseQLJSONEncoder`**: Added support for `UNSET` values by converting them to `None` during JSON serialization:

```python
def default(self, obj: Any) -> Any:
    # Handle UNSET (convert to None for JSON serialization)
    if obj is UNSET:
        return None
    # ... existing handlers
```

2. **Applied Custom Encoder to GraphQL Endpoints**: Configured the GraphQL POST endpoints to use `FraiseQLJSONResponse` which uses the enhanced encoder:

```python
@router.post("/graphql", response_class=FraiseQLJSONResponse)
async def graphql_endpoint(...):
```

## What Changed

### Files Modified:
- `src/fraiseql/fastapi/json_encoder.py`: Added UNSET handling and `FraiseQLJSONResponse` class
- `src/fraiseql/fastapi/routers.py`: Applied custom response class to GraphQL endpoints

### Behavior Changes:
- **Before**: GraphQL responses with UNSET values would fail with JSON serialization error
- **After**: UNSET values are automatically converted to `null` in JSON responses

### Example Error Response Handling:

**Before (failed)**:
```python
# This would throw: "Object of type Unset is not JSON serializable"
{
    "errors": [{
        "extensions": {
            "input": {
                "required_field": "value",
                "optional_field": UNSET  # Could not serialize
            }
        }
    }]
}
```

**After (works)**:
```json
{
    "errors": [{
        "extensions": {
            "input": {
                "required_field": "value",
                "optional_field": null
            }
        }
    }]
}
```

## Backward Compatibility

This fix is fully backward compatible:
- Existing code continues to work unchanged
- UNSET values in mutations still work correctly for SQL generation (excluded from JSON sent to PostgreSQL)
- Only the final JSON serialization behavior for GraphQL responses has changed
- Other FastAPI endpoints are unaffected (only GraphQL endpoints use the custom response class)

## Testing

Added comprehensive tests in `tests/test_unset_json_serialization.py` that verify:
- UNSET values are properly converted to `null` in JSON
- Nested structures with UNSET are handled correctly
- Error responses containing UNSET serialize successfully
- The custom response class works correctly

## Usage

No changes required for existing code. The fix is transparent:

```python
# This now works without any code changes needed
@fraiseql.input
class MyInput:
    required_field: str
    optional_field: str | None = UNSET  # ✅ Now serializes correctly in errors

@fraiseql.mutation(function="my_function", schema="app")
class MyMutation:
    input: MyInput
    success: MySuccess
    failure: MyError  # ✅ Error responses with UNSET now work
```

## Performance Impact

Minimal performance impact:
- Custom JSON encoder only processes UNSET values when they're present
- Only affects GraphQL endpoints, not other FastAPI routes
- Uses efficient identity check (`obj is UNSET`)

This fix resolves the issue reported in `FRAISEQL_UNSET_SERIALIZATION_ISSUE.md` and enables full use of the UNSET feature for distinguishing between null and omitted fields.
