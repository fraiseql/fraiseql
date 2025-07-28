# Query Type No Fields Error

## Issue

We're getting `TypeError: Type Query must define one or more fields` when trying to set up queries with FraiseQL v0.1.0a14.

## Current Setup

### Query Type Definition
```python
@fraiseql.type
class Query:
    """Root query type."""

    async def machines(
        self,
        info,
        limit: int = 20,
        offset: int = 0,
        where: Optional[MachineWhereInput] = None,
    ) -> list[Machine]:
        """Retrieve a list of machines."""
        db = info.context["db"]
        tenant_id = info.context.get("tenant_id")

        return await db.find("tv_machine",
            tenant_id=tenant_id,
            limit=limit,
            offset=offset,
            order_by="removed_at DESC NULLS LAST"
        )

    async def machine(self, info, id: uuid.UUID) -> Optional[Machine]:
        """Get a single machine by ID."""
        db = info.context["db"]
        tenant_id = info.context.get("tenant_id")

        return await db.find_one("tv_machine",
            id=id,
            tenant_id=tenant_id
        )
```

### App Creation
```python
QUERIES: list[type] = [Query]

fraiseql_app = create_fraiseql_app(
    config=fraiseql_config,
    types=TYPES,
    queries=QUERIES,
    mutations=MUTATIONS,
    context_getter=get_context,
)
```

## Error Stack Trace
```
File "/home/lionel/code/printoptim_backend/.venv/lib/python3.13/site-packages/fraiseql/gql/schema_builder.py", line 589, in build_fraiseql_schema
    query=registry.build_query_type(),
          ~~~~~~~~~~~~~~~~~~~~~~~~~^^
  File "/home/lionel/code/printoptim_backend/.venv/lib/python3.13/site-packages/fraiseql/gql/schema_builder.py", line 362, in build_query_type
    raise TypeError(msg)
TypeError: Type Query must define one or more fields.
```

## What We've Tried

1. Using `@fraiseql.field` decorator on methods - same error
2. Using `@fraiseql.query` decorator on standalone functions - FraiseQL doesn't find them
3. Different method signatures (with/without root parameter)
4. Naming the class both `Query` and `QueryRoot`

## Questions

1. What's the correct way to define query fields that FraiseQL will recognize?
2. Do we need specific decorators on the methods?
3. Is there a specific method signature FraiseQL expects?
4. Should we be using a different pattern entirely?

Please provide a minimal working example of how to define queries with FraiseQL v0.1.0a14.
