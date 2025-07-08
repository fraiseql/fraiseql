# Automatic Error Population for Mutation Error Types

As of FraiseQL v0.1.0b6, mutation error types that have an `errors` field will automatically have it populated with structured error information when the field is `None`.

## How It Works

When a mutation returns an error type (based on the `MutationErrorConfig`), FraiseQL will:

1. Create the error instance with all available fields
2. Check if the error type has an `errors` field that is `None`
3. If so, automatically populate it with a list containing a single error object

## Example

```python
import fraiseql
from fraiseql.mutations.error_config import MutationErrorConfig

# Define your error type
@fraiseql.type
class Error:
    message: str
    code: int
    identifier: str
    details: dict | None = None

# Define mutation error type with errors field
@fraiseql.failure
class CreateLocationError(MutationResultBase):
    errors: list[Error] | None = None  # Will be auto-populated if None
    conflict_location: Location | None = None

# Configure which statuses are treated as errors
config = MutationErrorConfig(
    error_prefixes={"noop:already_exists", "failed:"}
)

# Use in mutation
@fraiseql.mutation(
    function="create_location",
    error_config=config
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError
```

When the database returns a result like:
```json
{
  "status": "noop:already_exists",
  "message": "Location with this name already exists",
  "object_data": {"conflict_location": {...}},
  "extra_metadata": {"attempted_name": "Main Office"}
}
```

The error response will automatically include:
```json
{
  "__typename": "CreateLocationError",
  "errors": [{
    "message": "Location with this name already exists",
    "code": 409,
    "identifier": "already_exists",
    "details": {"attempted_name": "Main Office"}
  }],
  "conflict_location": {...}
}
```

## Status to Error Code Mapping

FraiseQL automatically maps status patterns to appropriate HTTP error codes:

- `not_found` → 404
- `unauthorized` → 401
- `forbidden` → 403
- `conflict`, `duplicate`, `exists` → 409
- `validation`, `invalid` → 422
- `timeout` → 408
- `noop:*` → 422 (Unprocessable Entity)
- `blocked:*` → 422 (Unprocessable Entity)
- `failed:*` → 500 (Internal Server Error)
- Default → 500

## Status to Identifier Extraction

The error identifier is extracted from the status:
- For prefixed statuses like `noop:already_exists`, the identifier is `already_exists`
- For other statuses, the full status is used with spaces/dashes replaced by underscores

## Customization

This feature works automatically with the default behavior. If you need custom error population logic, you can:

1. Pre-populate the `errors` field in your database function's `extra_metadata`
2. Create a custom error type without an `errors` field
3. Handle error population in a custom resolver

## Backward Compatibility

This feature is fully backward compatible:
- If the `errors` field is already populated (not None), it won't be overwritten
- If the error type doesn't have an `errors` field, nothing happens
- Success results are not affected