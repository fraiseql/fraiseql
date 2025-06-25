# Dual Mode Dev/Production Object Instantiation

## Overview

Implement a dual-mode system in FraiseQL where:
- **Development**: Full recursive instantiation of typed objects (better DX, type safety)
- **Production**: Return raw dict data from database (zero overhead, maximum performance)

This aligns with FraiseQL's philosophy of staying close to the database and minimizing abstraction overhead in production.

## Design Principles

1. **Zero overhead in production** - Raw database data passes through unchanged
2. **Rich development experience** - Full type safety and object navigation in dev
3. **Transparent to resolvers** - Mode switching happens at framework level
4. **Simple configuration** - Environment variable or config setting

## Proposed Implementation

### 1. Repository Level Implementation

Modify `FraiseQLRepository` to handle both modes:

```python
class FraiseQLRepository:
    def __init__(self, connection, context=None):
        self.connection = connection
        self.context = context or {}
        self.mode = self._determine_mode()
    
    def _determine_mode(self):
        """Determine if we're in dev or production mode."""
        # Check context first (allows per-request override)
        if 'mode' in self.context:
            return self.context['mode']
        
        # Then environment
        env = os.getenv('FRAISEQL_ENV', 'production')
        return 'development' if env == 'development' else 'production'
    
    async def find(self, view_name: str, **kwargs):
        """Find records with mode-appropriate return type."""
        rows = await self._execute_query(view_name, **kwargs)
        
        if self.mode == 'production':
            # Production: Return raw dicts
            return rows
        
        # Development: Full instantiation
        type_class = self._get_type_for_view(view_name)
        return [self._instantiate_recursive(type_class, row) for row in rows]
    
    async def find_one(self, view_name: str, **kwargs):
        """Find single record with mode-appropriate return type."""
        row = await self._execute_single_query(view_name, **kwargs)
        
        if not row:
            return None
            
        if self.mode == 'production':
            return row
            
        type_class = self._get_type_for_view(view_name)
        return self._instantiate_recursive(type_class, row)
    
    def _instantiate_recursive(self, type_class, data, cache=None, depth=0):
        """Recursively instantiate nested objects (dev mode only)."""
        if cache is None:
            cache = {}
        
        # Check cache for circular references
        if isinstance(data, dict) and 'id' in data:
            obj_id = data['id']
            if obj_id in cache:
                return cache[obj_id]
        
        # Max recursion check
        if depth > 10:
            raise ValueError(f"Max recursion depth exceeded for {type_class.__name__}")
        
        # Convert camelCase to snake_case
        snake_data = {}
        for key, value in data.items():
            if key == '__typename':
                continue
            snake_key = to_snake_case(key)
            
            # Check if this field should be recursively instantiated
            if isinstance(value, dict) and snake_key in type_class.__gql_type_hints__:
                field_type = type_class.__gql_type_hints__[snake_key]
                # Extract the actual type from Optional, List, etc.
                actual_type = self._extract_type(field_type)
                if hasattr(actual_type, '__fraiseql_definition__'):
                    value = self._instantiate_recursive(actual_type, value, cache, depth + 1)
            elif isinstance(value, list) and snake_key in type_class.__gql_type_hints__:
                field_type = type_class.__gql_type_hints__[snake_key]
                item_type = self._extract_list_type(field_type)
                if item_type and hasattr(item_type, '__fraiseql_definition__'):
                    value = [
                        self._instantiate_recursive(item_type, item, cache, depth + 1)
                        for item in value
                    ]
            
            snake_data[snake_key] = value
        
        # Create instance
        instance = type_class(**snake_data)
        
        # Cache it
        if 'id' in data:
            cache[data['id']] = instance
        
        return instance
```

### 2. Resolver Pattern

Resolvers remain clean and mode-agnostic:

```python
@fraiseql.type
class Query:
    @fraiseql.field
    async def allocations(self, info: fraiseql.Info) -> list[Allocation]:
        """Get all allocations."""
        repo = FraiseQLRepository(info.context["db"], info.context)
        return await repo.find("tv_allocation")
    
    @fraiseql.field
    async def allocation(self, info: fraiseql.Info, id: UUID) -> Allocation | None:
        """Get single allocation."""
        repo = FraiseQLRepository(info.context["db"], info.context)
        return await repo.find_one("tv_allocation", id=id)
```

### 3. Type System Integration

Ensure types work with both modes:

```python
@fraiseql.type
class Allocation:
    id: UUID
    machine_id: UUID | None
    data: dict[str, Any]  # Raw JSONB data
    
    # These work in both modes:
    # - Dev: machine is a Machine instance
    # - Prod: machine is a dict
    @fraiseql.field
    async def machine(self, info) -> Machine | dict[str, Any] | None:
        """Get the allocated machine."""
        if hasattr(self, '_machine'):
            # Dev mode: already instantiated
            return self._machine
        # Prod mode: extract from data
        return self.data.get('machine')
```

### 4. Configuration

Add to FraiseQL configuration:

```python
class FraiseQLConfig:
    def __init__(
        self,
        environment: Literal["development", "production"] = "production",
        auto_instantiate_nested: bool | None = None,
        ...
    ):
        self.environment = environment
        # Allow explicit override
        if auto_instantiate_nested is None:
            self.auto_instantiate_nested = (environment == "development")
        else:
            self.auto_instantiate_nested = auto_instantiate_nested
```

### 5. Context Integration

Allow per-request mode override:

```python
async def build_graphql_context(request: Request, db_connection) -> dict:
    """Build GraphQL context with mode detection."""
    context = {
        "db": db_connection,
        "request": request,
    }
    
    # Allow header-based override for testing
    if "X-FraiseQL-Mode" in request.headers:
        context["mode"] = request.headers["X-FraiseQL-Mode"]
    
    return context
```

## Benefits

1. **Production Performance**: Zero overhead - data flows directly from DB to client
2. **Development Experience**: Full type safety and IDE support
3. **Gradual Adoption**: Can be enabled/disabled per deployment
4. **Testing**: Can test both modes easily
5. **Debugging**: Can enable dev mode in production for debugging

## Implementation Steps

1. Add mode detection to `FraiseQLRepository`
2. Implement `_instantiate_recursive` method
3. Add type extraction utilities for Optional, List, etc.
4. Update config to support environment setting
5. Add tests for both modes
6. Document the behavior difference

## Example Usage

```python
# Development mode output
allocation = await repo.find_one("tv_allocation", id=some_id)
print(type(allocation))  # <class 'Allocation'>
print(type(allocation.machine))  # <class 'Machine'>
print(allocation.machine.name)  # "Printer XYZ"

# Production mode output  
allocation = await repo.find_one("tv_allocation", id=some_id)
print(type(allocation))  # <class 'dict'>
print(type(allocation['machine']))  # <class 'dict'>
print(allocation['machine']['name'])  # "Printer XYZ"
```

## Testing Strategy

```python
@pytest.mark.parametrize("mode", ["development", "production"])
async def test_allocation_query_modes(mode):
    """Test that both modes work correctly."""
    context = {"mode": mode}
    repo = FraiseQLRepository(db, context)
    
    result = await repo.find("tv_allocation")
    
    if mode == "development":
        assert isinstance(result[0], Allocation)
        assert isinstance(result[0].machine, Machine)
    else:
        assert isinstance(result[0], dict)
        assert isinstance(result[0]['machine'], dict)
```

## Future Enhancements

1. **Partial instantiation**: Configure which types to instantiate
2. **Lazy instantiation**: Instantiate objects on first access
3. **Performance monitoring**: Track instantiation overhead
4. **Caching**: Reuse instantiated objects across requests