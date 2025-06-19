# Migration Guide: From Decorators to Explicit Registration

This guide helps you migrate from FraiseQL's decorator-based registration to the explicit registration pattern, which provides better testability and eliminates import order dependencies.

## Why Migrate?

The decorator-based approach has several limitations:

1. **Import Order Dependencies**: Decorators register at import time, requiring careful import ordering
2. **Global State**: Decorators use a global registry, making testing difficult
3. **Hidden Dependencies**: Dependencies aren't explicit in the code
4. **Test Isolation**: Tests can contaminate each other through shared global state

## Benefits of Explicit Registration

- **No Import Order Issues**: Registration happens explicitly when you want it
- **Better Testing**: Each test can use an isolated registry
- **Explicit Dependencies**: All dependencies are visible in function signatures
- **Modular Organization**: Easy to organize code by feature

## Migration Steps

### Step 1: Install Enhanced Modules

First, ensure you have the enhanced registry modules:

```python
from fraiseql.mutations.registry_v2 import ScopedResultRegistry
from fraiseql.mutations.decorators_v2 import create_mutations
```

### Step 2: Convert Decorator-Based Code

#### Before (Decorator-Based):

```python
import fraiseql
from fraiseql.mutations import mutation

@fraiseql.type
class CreateUserSuccess:
    user: User
    message: str

@fraiseql.type
class CreateUserError:
    code: str
    message: str

# This registers globally at import time
@mutation
async def create_user(input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
    # Implementation
    pass
```

#### After (Explicit Registration):

```python
import fraiseql
from fraiseql.mutations.registry_v2 import ScopedResultRegistry
from fraiseql.mutations.decorators_v2 import create_mutations

@fraiseql.type
class CreateUserSuccess:
    user: User
    message: str

@fraiseql.type
class CreateUserError:
    code: str
    message: str

def setup_mutations(registry: ScopedResultRegistry):
    """Set up mutations with explicit registration."""
    builder = create_mutations(registry)
    
    @builder.mutation(
        result_type=CreateUserSuccess,
        error_type=CreateUserError,
        sql_function="users.create_user"  # Optional: specify SQL function
    )
    async def create_user(input: CreateUserInput):
        # Implementation
        pass
    
    return builder
```

### Step 3: Update Application Initialization

#### Before:

```python
# main.py
from fraiseql import FraiseQL

# Import all modules with decorators (order matters!)
import users.mutations
import posts.mutations
import comments.mutations

# Create app after all imports
app = FraiseQL()
```

#### After:

```python
# main.py
from fraiseql import FraiseQL
from fraiseql.mutations.registry_v2 import ScopedResultRegistry

# Import setup functions (order doesn't matter)
from users.mutations import setup_user_mutations
from posts.mutations import setup_post_mutations
from comments.mutations import setup_comment_mutations

# Create app with explicit registration
def create_app():
    registry = ScopedResultRegistry()
    
    # Register all mutations explicitly
    user_builder = setup_user_mutations(registry)
    post_builder = setup_post_mutations(registry)
    comment_builder = setup_comment_mutations(registry)
    
    # Create app with configured registry
    app = FraiseQL(registry=registry)
    
    return app
```

### Step 4: Update Tests

#### Before (Contaminated Global State):

```python
import pytest
from myapp.mutations import create_user

@pytest.mark.asyncio
async def test_create_user():
    # This test affects global state
    result = await create_user(...)
    assert result...

@pytest.mark.asyncio  
async def test_create_user_error():
    # This test might be affected by previous test
    result = await create_user(...)
    assert result...
```

#### After (Isolated Tests):

```python
import pytest
from fraiseql.mutations.registry_v2 import isolated_registry
from myapp.mutations import setup_mutations

@pytest.mark.asyncio
async def test_create_user():
    # Each test gets its own registry
    with isolated_registry() as registry:
        builder = setup_mutations(registry)
        create_user = builder.get_mutation("create_user").function
        
        result = await create_user(...)
        assert result...

@pytest.mark.asyncio
async def test_create_user_error():
    # Completely isolated from other tests
    with isolated_registry() as registry:
        builder = setup_mutations(registry)
        create_user = builder.get_mutation("create_user").function
        
        result = await create_user(...)
        assert result...
```

## Common Patterns

### Pattern 1: Module Organization

Organize mutations by feature:

```python
# users/mutations.py
def setup_user_mutations(registry):
    builder = create_mutations(registry)
    
    @builder.mutation(...)
    async def create_user(...): ...
    
    @builder.mutation(...)
    async def update_user(...): ...
    
    @builder.mutation(...)
    async def delete_user(...): ...
    
    return builder
```

### Pattern 2: Dependency Injection

Pass dependencies explicitly:

```python
def setup_mutations(registry, config, services):
    builder = create_mutations(registry)
    
    @builder.mutation(...)
    async def create_user(input, repo=services.repo):
        # Use injected services
        pass
    
    return builder
```

### Pattern 3: Testing Helpers

Create test utilities:

```python
# tests/helpers.py
from contextlib import contextmanager

@contextmanager
def test_mutations(*setup_funcs):
    """Helper for testing mutations."""
    with isolated_registry() as registry:
        builders = []
        for setup_func in setup_funcs:
            builder = setup_func(registry)
            builders.append(builder)
        
        yield registry, builders
```

## Gradual Migration

You can migrate gradually:

1. **Phase 1**: Add new mutations using explicit registration
2. **Phase 2**: Convert existing mutations module by module
3. **Phase 3**: Remove old decorator imports
4. **Phase 4**: Switch to explicit app initialization

## Troubleshooting

### Issue: "Type not registered"

**Solution**: Ensure the setup function is called before schema creation:

```python
# Correct order
registry = ScopedResultRegistry()
setup_mutations(registry)  # Register first
app = FraiseQL(registry=registry)  # Then create app
```

### Issue: "Import order still matters"

**Solution**: Use setup functions instead of module-level code:

```python
# Bad: Module-level registration
builder = create_mutations(registry)  # This runs at import

# Good: Function-based registration  
def setup_mutations(registry):
    builder = create_mutations(registry)  # This runs when called
    return builder
```

### Issue: "Tests affect each other"

**Solution**: Use `isolated_registry()` for each test:

```python
# Each test is completely isolated
with isolated_registry() as registry:
    # Test code here
```

## Next Steps

After migration:

1. Remove unused decorator imports
2. Update documentation to use explicit registration
3. Add type hints for better IDE support
4. Consider using dependency injection frameworks

The explicit registration pattern provides a more maintainable and testable codebase while preserving all the functionality of the decorator-based approach.