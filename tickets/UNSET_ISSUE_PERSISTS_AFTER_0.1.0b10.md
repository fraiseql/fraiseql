# UNSET Serialization Issue Persists After v0.1.0b10 Upgrade

**Date:** 2025-07-11
**Reporter:** PrintOptim Backend Team
**FraiseQL Version:** 0.1.0b10 (freshly reinstalled)
**Status:** Critical - Still blocking 137+ tests

## Issue Update

Despite upgrading to FraiseQL v0.1.0b10 as recommended, the UNSET serialization issue persists. The exact same error continues to occur.

## Steps Taken

1. **Verified version**: `uv pip show fraiseql` confirms Version: 0.1.0b10
2. **Force reinstalled**: `uv pip install --force-reinstall fraiseql==0.1.0b10`
3. **Ran test again**: Same error persists

## Current Error

```
GraphQL errors: [{'message': 'Object of type Unset is not JSON serializable', 'locations': [{'line': 3, 'column': 13}], 'path': ['createMachineItem'], 'extensions': {}}]
```

## Test Command

```bash
uv run pytest tests/api/test_machine_item_mutations.py::test_create_machine_item_with_order -xvs
```

## Mutation Being Tested

```graphql
mutation CreateMachineItem($input: CreateMachineItemInput!) {
    createMachineItem(input: $input) {
        ... on CreateMachineItemSuccess {
            status
            machineItem {
                id
                installedAt
            }
        }
        ... on CreateMachineItemError {
            status
            errors {
                code
                identifier
                message
            }
        }
    }
}
```

## Input Type Definition

```python
@fraiseql.input
class CreateMachineItemInput:
    machine_id: uuid.UUID
    source_id: uuid.UUID
    source_type: MachineItemSourceType
    installed_at: datetime | None = UNSET
    order_id: uuid.UUID | None = UNSET
    order_data: CreateOrderInlineInput | None = UNSET
```

## Input Data Sent

```python
{
    "machineId": "00000000-0000-0000-0000-000000000000",  # Invalid ID to trigger error
    "sourceId": "11111111-1111-1111-1111-111111111111",
    "sourceType": "PRODUCT",
    "orderId": "33333333-3333-3333-3333-333333333333",
}
```

## Expected Behavior

According to the v0.1.0b10 release notes, UNSET values should be automatically handled without any code changes needed.

## Additional Context

- We're using `create_fraiseql_app()` to set up the GraphQL endpoint
- We have not applied any custom JSON response classes
- The error occurs when the mutation tries to return an error (due to invalid machine ID)

## Questions

1. Are there any additional configuration steps needed beyond upgrading to v0.1.0b10?
2. Does `create_fraiseql_app()` automatically apply the FraiseQLJSONResponse class?
3. Could there be a specific interaction with our error configuration (`PRINTOPTIM_ERROR_CONFIG`)?

## Request

Please investigate why the fix in v0.1.0b10 is not working in our specific case. We're happy to provide any additional debugging information needed.
