# FraiseQL NULL vs UNSET Fix

## Summary

FraiseQL now properly distinguishes between NULL and UNSET fields in GraphQL mutations. This fix addresses the issue where omitted fields were being sent as `null` to PostgreSQL functions, preventing proper validation of mutually exclusive fields.

## What Changed

The `coerce_input_arguments` function in `src/fraiseql/types/coercion.py` was updated to preserve the distinction between:
- **Omitted fields**: Not included in the GraphQL request → Use field's default value
- **Explicit null**: Field explicitly set to `null` in the request → Value is `None`

## How to Use

### Define Input Types with UNSET Defaults

```python
from fraiseql import fraise_field
from fraiseql.types.definitions import UNSET

@fraiseql.input
class CreateMachineItemInput:
    machine_id: uuid.UUID
    source_type: SourceType
    source_id: uuid.UUID
    
    # Optional fields that should be omitted when not provided
    order_id: uuid.UUID | None = UNSET
    order_data: CreateOrderInlineInput | None = UNSET
```

### PostgreSQL Validation Works Correctly

Now your PostgreSQL functions can properly check field presence:

```sql
-- This validation now works as expected
IF (v_fields ? 'order_id') AND (v_fields ? 'order_data') THEN
    RAISE EXCEPTION 'Cannot provide both order_id and order_data';
END IF;
```

### Behavior Examples

1. **Omitted field** (not in GraphQL request):
   ```graphql
   mutation {
     createMachineItem(input: {
       machineId: "123"
       sourceType: PRODUCT
       sourceId: "456"
       # order_id and order_data are omitted
     }) {
       __typename
     }
   }
   ```
   PostgreSQL receives:
   ```json
   {
     "machine_id": "123",
     "source_type": "Product",
     "source_id": "456"
   }
   ```

2. **Explicit null** (field set to null):
   ```graphql
   mutation {
     createMachineItem(input: {
       machineId: "123"
       sourceType: PRODUCT
       sourceId: "456"
       orderId: null  # Explicitly set to null
     }) {
       __typename
     }
   }
   ```
   PostgreSQL receives:
   ```json
   {
     "machine_id": "123",
     "source_type": "Product", 
     "source_id": "456",
     "order_id": null
   }
   ```

## Migration Guide

If you have existing input types with `None` defaults that you want to be omitted when not provided:

**Before:**
```python
@fraiseql.input
class MyInput:
    optional_field: str | None = None  # Sent as null when omitted
```

**After:**
```python
from fraiseql.types.definitions import UNSET

@fraiseql.input 
class MyInput:
    optional_field: str | None = UNSET  # Omitted when not provided
```

## Testing

A comprehensive test suite has been added in `tests/test_null_vs_unset_coercion.py` that verifies:
- Omitted fields use their default values
- Explicit null overrides defaults
- UNSET fields are excluded from SQL generation
- The behavior works correctly through the full GraphQL → SQL pipeline

## Compatibility

This change is backward compatible. Existing code will continue to work as before. The new behavior only applies when you explicitly use `UNSET` as a default value.