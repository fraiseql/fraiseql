# Response: JSONB Nested Objects in FraiseQL

## Summary

FraiseQL does **not** automatically instantiate typed objects from JSONB data. The nested objects remain as dictionaries when queried. However, FraiseQL provides flexible options for handling this scenario.

## Recommended Approaches

### Option 1: Direct Dictionary Access (Simplest)
Keep the `data` field as `dict[str, Any]` and let GraphQL clients query nested fields directly:

```python
@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine_id: uuid.UUID | None
    # ... other fields ...
    data: dict[str, Any]  # Contains machine, location, etc.
```

GraphQL queries can access nested data:
```graphql
query {
  allocations {
    id
    data {
      machine {
        id
        name
      }
      location {
        name
      }
    }
  }
}
```

### Option 2: Custom Field Resolvers (Type-Safe)
Use `@fraiseql.field` to extract and optionally transform nested objects:

```python
@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine_id: uuid.UUID | None
    # ... other fields ...
    data: dict[str, Any]
    
    @fraiseql.field
    async def machine(self, info) -> dict[str, Any] | None:
        """Extract machine from JSONB data."""
        return self.data.get("machine")
    
    @fraiseql.field
    async def location(self, info) -> dict[str, Any] | None:
        """Extract location from JSONB data."""
        return self.data.get("location")
    
    @fraiseql.field
    async def organizational_unit(self, info) -> dict[str, Any] | None:
        """Extract organizational unit from JSONB data."""
        return self.data.get("organizational_unit")
    
    @fraiseql.field
    async def network_configuration(self, info) -> dict[str, Any] | None:
        """Extract network configuration from JSONB data."""
        return self.data.get("network_configuration")
```

### Option 3: Typed Field Resolvers (Most Type-Safe)
If you have defined types for Machine, Location, etc., you can return them from field resolvers:

```python
@fraiseql.type
class Machine:
    id: uuid.UUID
    name: str
    # ... other fields ...

@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine_id: uuid.UUID | None
    # ... other fields ...
    data: dict[str, Any]
    
    @fraiseql.field
    async def machine(self, info) -> Machine | None:
        """Extract and transform machine from JSONB data."""
        machine_data = self.data.get("machine")
        if machine_data:
            return Machine(**machine_data)  # Or use from_dict if available
        return None
```

## Important Notes

1. **No Automatic Type Instantiation**: The comment about "automatic instantiation" was likely referring to FraiseQL's ability to automatically generate SQL with JSONB path extraction, not automatic Python object creation.

2. **Query Translation**: FraiseQL automatically translates nested GraphQL queries into appropriate JSONB extraction SQL (`data->'machine'->>'id'`), so nested queries work out of the box.

3. **Performance**: Using field resolvers (Options 2 & 3) doesn't impact database performance - the JSONB extraction happens in PostgreSQL regardless of approach.

4. **Type Safety**: Option 3 provides the best type safety but requires more code. Option 1 is simplest but provides no type checking for nested data.

## Recommendation

For PrintOptim, I recommend **Option 2** as it provides:
- Clean GraphQL schema matching the old Strawberry implementation
- No need to expose the internal `data` field to GraphQL clients
- Flexibility to add validation or transformation later
- Good balance between simplicity and type safety

The field resolvers are simple property accessors that extract data from the JSONB column, maintaining backward compatibility with the previous API while leveraging FraiseQL's JSONB capabilities.