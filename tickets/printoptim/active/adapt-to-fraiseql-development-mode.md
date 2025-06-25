# Adapt PrintOptim to Use FraiseQL Development Mode

## Context

FraiseQL v0.1.0a13 introduces a dual-mode repository feature that allows different behavior in development vs production environments. This ticket outlines the changes needed for PrintOptim to leverage development mode for better developer experience.

## Current Behavior

PrintOptim currently uses FraiseQL repositories that return raw dictionary data:

```python
# Current usage - returns dicts
repo = FraiseQLRepository(pool)
machines = await repo.run("SELECT * FROM machine_view")
# Access as dict: machines[0]["name"]
```

## Required Changes

### 1. Enable Development Mode

Set the environment variable in your development environment:

```bash
# In .env or development environment
export FRAISEQL_ENV=development
```

Or configure per-repository instance:

```python
# In development configuration
repo = FraiseQLRepository(pool, context={"mode": "development"})
```

### 2. Use New Repository Methods

Replace direct SQL queries with the new typed methods:

```python
# OLD: Direct SQL query returning dicts
machines = await repo.run("SELECT * FROM machine_view WHERE status = 'active'")

# NEW: Typed method returning Machine objects in dev mode
machines = await repo.find("machine_view", status="active")
machine = await repo.find_one("machine_view", id=machine_id)
```

### 3. Update Type Definitions

Ensure your FraiseQL types are properly decorated:

```python
from fraiseql import fraise_type, fraise_field

@fraise_type
class Machine:
    id: str
    name: str
    status: str
    allocations: list[Allocation] = fraise_field(default_factory=list)

@fraise_type
class Allocation:
    id: str
    machine_id: str
    machine: Optional[Machine] = None
    start_time: datetime
    end_time: Optional[datetime] = None
```

### 4. Benefits in Development

With development mode enabled:

```python
# Type-safe access with IDE autocomplete
machine = await repo.find_one("machine_view", id=machine_id)
print(machine.name)  # IDE knows about .name attribute
print(machine.allocations[0].start_time)  # Nested objects work

# Type checking works
if isinstance(machine, Machine):  # True in dev mode
    process_machine(machine)  # Type checker happy
```

### 5. Production Behavior

In production (default), the same code returns dicts for zero overhead:

```python
# Production mode - returns raw dicts
machine = await repo.find_one("machine_view", id=machine_id)
print(machine["name"])  # Dict access
print(isinstance(machine, dict))  # True in production
```

## Implementation Steps

1. **Update Development Environment**
   - Add `FRAISEQL_ENV=development` to development .env file
   - Ensure this is NOT set in production

2. **Refactor Repository Usage**
   - Find all `repo.run()` calls that query views
   - Replace with `repo.find()` or `repo.find_one()`
   - Update view names to match your database views

3. **Update Type Definitions**
   - Ensure all types use `@fraise_type` decorator
   - Add proper type hints for nested relationships
   - Use `fraise_field()` for default values

4. **Test Both Modes**
   - Run tests with `FRAISEQL_ENV=development`
   - Run tests without the env var (production mode)
   - Ensure code works in both modes

## Example Migration

```python
# Before
async def get_available_machines(repo: FraiseQLRepository) -> list[dict]:
    query = """
    SELECT * FROM machine_view 
    WHERE status = 'available' 
    ORDER BY name
    """
    machines = await repo.run(query)
    return machines

# After
async def get_available_machines(repo: FraiseQLRepository) -> list[Union[dict, Machine]]:
    machines = await repo.find("machine_view", status="available")
    # In dev: returns list[Machine] with type safety
    # In prod: returns list[dict] for performance
    return sorted(machines, key=lambda m: m.name if hasattr(m, 'name') else m['name'])
```

## Testing Recommendations

1. Create a test that verifies both modes:

```python
def test_dual_mode_compatibility():
    # Test with dev mode
    dev_repo = FraiseQLRepository(pool, {"mode": "development"})
    machine = await dev_repo.find_one("machine_view", id="test-id")
    assert isinstance(machine, Machine)
    assert hasattr(machine, 'name')
    
    # Test with prod mode
    prod_repo = FraiseQLRepository(pool, {"mode": "production"})
    machine = await prod_repo.find_one("machine_view", id="test-id")
    assert isinstance(machine, dict)
    assert 'name' in machine
```

## Timeline

- Update to FraiseQL v0.1.0a13: Immediate
- Environment configuration: 1 day
- Repository method migration: 2-3 days
- Testing both modes: 1 day

## Questions/Support

- FraiseQL documentation: https://github.com/fraiseql/fraiseql
- For questions about type definitions, check the test examples in FraiseQL
- The dual-mode feature is backward compatible - existing code continues to work