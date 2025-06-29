# FraiseQL: Query Decorator Not Registering - contracts Query Missing

## Issue Description
A query function decorated with `@fraiseql.query` is not being registered in the GraphQL schema, resulting in a "Cannot query field" error even though the query is implemented.

## Example Error
```
{
  "errors": [{
    "message": "Cannot query field 'contracts' on type 'Query'. Did you mean 'contacts' or 'contact'?",
    "locations": [{"line": 3, "column": 13}],
    "path": null
  }]
}
```

## Expected Behavior
The `contracts` query should be available in the GraphQL schema since it's implemented with the `@fraiseql.query` decorator.

## Actual Behavior
- FraiseQL reports that the `contracts` query doesn't exist
- It suggests similar fields: `contacts` or `contact` (which ARE working)
- The contracts query is not accessible via GraphQL

## Implementation Details
The contracts query is properly implemented in `gql_agreement_query.py`:

```python
@fraiseql.query
async def contracts(
    info: GraphQLResolveInfo,
    limit: int = 20,
    offset: int = 0,
    is_current: bool | None = None,
    organization_id: uuid.UUID | None = None,
) -> list[Contract]:
    """Retrieve a list of contracts with optional filtering."""
    # ... implementation
```

The module is imported BEFORE the FraiseQL app is created:
```python
# Line 276
from printoptim_backend.entrypoints.api.resolvers.query.dim.agreement import gql_agreement_query

# Line 356
fraiseql_app = create_fraiseql_app(
    config=fraiseql_config,
    types=TYPES,
    mutations=MUTATIONS,
    context_getter=get_context,
)
```

## Other Observations
- The `contact` and `contacts` queries from `gql_user_query.py` ARE working
- The Contract type is included in the TYPES list
- Other queries like `machine` and `allocation` are working
- Test `test_context_with_mutations` passes, but queries fail

## Possible Causes
1. The `@fraiseql.query` decorator might not be executing for some modules
2. There might be a timing issue with decorator registration
3. Import order might matter more than expected
4. The auto-discovery of queries might be missing some modules

## Environment
- FraiseQL version: 0.1.0b1
- Test failing in CI but might work locally