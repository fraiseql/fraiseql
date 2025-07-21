# FraiseQL Query Auto-Discovery Investigation Response

## Investigation Summary

I've thoroughly investigated the FraiseQL query auto-discovery issue reported in the PrintOptim backend. Here's what I found and the debugging enhancements I've added to help diagnose the problem.

## How FraiseQL Query Registration Works

1. **Decorator Pattern**: The `@fraiseql.query` decorator (in `src/fraiseql/decorators.py`) registers query functions with a singleton `SchemaRegistry`
2. **Singleton Registry**: `SchemaRegistry` (in `src/fraiseql/gql/builders/registry.py`) maintains dictionaries of registered queries, types, mutations, etc.
3. **Schema Building**: When `build_fraiseql_schema()` is called, it uses `QueryTypeBuilder` to convert registered queries into GraphQL fields

## Key Findings

### 1. Registration Mechanism is Sound
The core registration mechanism works correctly. When `@fraiseql.query` is applied to a function:
- The decorator gets the singleton registry instance
- It calls `registry.register_query(func)`
- The function is stored in `registry._queries` dictionary

### 2. Potential Issues Identified

#### A. Registry Clearing
In the PrintOptim `app.py`, line 282:
```python
SchemaRegistry._instance = None
```
This forcefully clears the singleton instance. Any queries registered before this line will be lost.

#### B. Import Order Matters
Queries are registered when the Python module is imported and the decorator is executed. The timing of imports relative to registry clearing is critical.

#### C. No Import Errors Reported
If a module fails to import due to missing dependencies or syntax errors, the queries won't be registered, but this might fail silently.

## Enhanced Debug Logging

I've added comprehensive debug logging to help diagnose the issue:

### 1. Decorator Logging (`src/fraiseql/decorators.py`)
```python
logger.debug(
    "Query decorator called for function '%s' in module '%s'",
    func.__name__,
    func.__module__
)
logger.debug(
    "Total queries registered after '%s': %d",
    func.__name__,
    len(registry.queries)
)
```

### 2. Registry Logging (`src/fraiseql/gql/builders/registry.py`)
```python
# Logs when registry instance is created or reused
logger.debug("Creating new SchemaRegistry instance")

# Logs query registration with module info
logger.debug(
    "Registering query '%s' from module '%s'",
    name,
    query_fn.__module__
)

# Warns about duplicate registrations
logger.warning(
    "Query '%s' is being overwritten. Previous module: %s, New module: %s",
    name,
    previous_module,
    new_module
)
```

### 3. Query Builder Logging (`src/fraiseql/gql/builders/query_builder.py`)
```python
# Shows all queries being processed
logger.debug(
    "Building query fields. Found %d registered queries: %s",
    len(self.registry.queries),
    list(self.registry.queries.keys())
)

# Confirms successful field creation
logger.debug(
    "Successfully added query field '%s' (GraphQL name: '%s') from function '%s'",
    name,
    graphql_field_name,
    fn.__module__
)
```

## Recommendations for PrintOptim Team

### 1. Enable Debug Logging
Add this to your application startup to see the registration flow:
```python
import logging
logging.basicConfig(level=logging.DEBUG)
logging.getLogger("fraiseql").setLevel(logging.DEBUG)
```

### 2. Check Import Success
Verify that the mat module is actually being imported successfully:
```python
try:
    import printoptim_backend.entrypoints.api.resolvers.query.dim.mat.gql_mat_query
    print("Mat module imported successfully")
except Exception as e:
    print(f"Failed to import mat module: {e}")
    import traceback
    traceback.print_exc()
```

### 3. Verify Registry State
After all imports, check what's in the registry:
```python
from fraiseql.gql.schema_builder import SchemaRegistry
registry = SchemaRegistry.get_instance()
print(f"Registered queries: {list(registry.queries.keys())}")
print(f"Total queries: {len(registry.queries)}")
```

### 4. Consider Import Order
Move the registry clearing to happen BEFORE any module imports:
```python
# Clear registry first
from fraiseql.gql.schema_builder import SchemaRegistry
SchemaRegistry._instance = None

# Then import all modules
from printoptim_backend.entrypoints.api.resolvers import queries
import printoptim_backend.entrypoints.api.resolvers.query.dim.mat.gql_mat_query
# ... other imports
```

### 5. Alternative: Explicit Registration
Instead of relying on import-time registration, you could explicitly register queries:
```python
from fraiseql import build_fraiseql_schema
from printoptim_backend.entrypoints.api.resolvers.query.dim.mat.gql_mat_query import (
    accessories, accessory, machine_items, machine_item
)

schema = build_fraiseql_schema(
    query_types=[accessories, accessory, machine_items, machine_item]
)
```

## Debugging Steps

1. **Run with debug logging enabled** to see the registration flow
2. **Check for import errors** in the mat module
3. **Verify the mat module functions have the @fraiseql.query decorator**
4. **Ensure type annotations are present** on all query functions
5. **Look for any circular imports** that might prevent module loading

## Version Consideration

You mentioned having v0.1.0b25 installed but v0.1.0b22 showing in logs. This version mismatch could potentially cause issues if the decorator behavior changed between versions. Ensure you're using a consistent version throughout.

## Next Steps

With the debug logging in place, you should be able to see:
- When each query is registered
- Which module it comes from
- If any queries are being overwritten
- The final state of the registry before schema building

This information will help pinpoint exactly where the mat queries are getting lost in the registration process.
