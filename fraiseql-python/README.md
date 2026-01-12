# FraiseQL v2 - Python Schema Authoring

**Python decorators for authoring FraiseQL schemas**

This package provides Python decorators to define GraphQL schemas that are compiled by the FraiseQL Rust engine.

## Architecture

```
Python Decorators → schema.json → fraiseql-cli compile → schema.compiled.json → Rust Runtime
```

**Important**: This package is for **schema authoring only**. It does NOT provide runtime execution.
The compiled schema is executed by the standalone Rust server.

## Installation

```bash
pip install fraiseql
```

## Quick Start

```python
import fraiseql

# Define a GraphQL type
@fraiseql.type
class User:
    id: int
    name: str
    email: str
    created_at: str

# Define a query
@fraiseql.query(sql_source="v_user")
def users(limit: int = 10) -> list[User]:
    """Get all users with pagination."""
    pass

# Define a mutation
@fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
def create_user(name: str, email: str) -> User:
    """Create a new user."""
    pass

# Export schema to JSON
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

## Compile Schema

```bash
# Compile schema.json to optimized schema.compiled.json
fraiseql-cli compile schema.json -o schema.compiled.json

# Start server with compiled schema
fraiseql-server --schema schema.compiled.json
```

## Features

- **Type-safe**: Python type hints map to GraphQL types
- **Database-backed**: Queries map to SQL views, mutations to functions
- **Compile-time**: All validation happens at compile time, zero runtime overhead
- **No FFI**: Pure JSON output, no Python-Rust bindings needed

## Type Mapping

| Python Type | GraphQL Type |
|-------------|--------------|
| `int` | `Int` |
| `float` | `Float` |
| `str` | `String` |
| `bool` | `Boolean` |
| `list[T]` | `[T]` |
| `T \| None` | `T` (nullable) |
| Custom class | Object type |

## Documentation

Full documentation: https://fraiseql.readthedocs.io

## License

MIT
