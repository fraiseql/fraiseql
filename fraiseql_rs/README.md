# fraiseql-rs

**Ultra-fast GraphQL JSON transformation in Rust**

A high-performance Python extension module for transforming JSON data from snake_case database formats to camelCase GraphQL responses with automatic `__typename` injection.

**📍 You are here: Rust Core Implementation (Required)**

**Relationship to Main Project**: Core Rust implementation providing the base JSON transformation engine. Required for FraiseQL's performance. See [root README](../README.md) for main framework.

## Features

- **🚀 7-10x faster** than pure Python implementations
- **Zero-copy JSON parsing** with serde_json
- **Automatic type detection** from GraphQL-like schemas
- **GIL-free execution** for true parallelism
- **Schema-aware transformations** with nested array support
- **Reusable schema registry** for optimal performance

## Installation

```bash
# Development installation
maturin develop

# Production build
maturin build --release
```

## Quick Start

```python
import fraiseql_rs
import json

# Simple transformation
input_json = '{"user_id": 1, "user_name": "John"}'
result = fraiseql_rs.transform_json(input_json)
# → '{"userId":1,"userName":"John"}'

# With __typename injection
result = fraiseql_rs.transform_json_with_typename(input_json, "User")
# → '{"__typename":"User","userId":1,"userName":"John"}'

# Schema-aware transformation (recommended)
schema = {
    "User": {
        "fields": {
            "id": "Int",
            "name": "String",
            "posts": "[Post]"
        }
    },
    "Post": {
        "fields": {
            "id": "Int",
            "title": "String"
        }
    }
}

result = fraiseql_rs.transform_with_schema(input_json, "User", schema)
# → Automatic __typename at all levels, including arrays
```

## API Overview

### Core Functions

#### `to_camel_case(s: str) -> str`
Convert a single snake_case string to camelCase.

```python
fraiseql_rs.to_camel_case("user_name")  # → "userName"
```

#### `transform_keys(obj: dict, recursive: bool = False) -> dict`
Transform dictionary keys from snake_case to camelCase.

```python
data = {"user_id": 1, "user_name": "John"}
fraiseql_rs.transform_keys(data)  # → {"userId": 1, "userName": "John"}
```

#### `transform_json(json_str: str) -> str`
Transform JSON string with camelCase conversion. **Fastest option** when no type information is needed.

```python
input_json = '{"user_id": 1, "user_posts": [{"post_id": 1}]}'
result = fraiseql_rs.transform_json(input_json)
# → '{"userId":1,"userPosts":[{"postId":1}]}'
```

#### `transform_json_with_typename(json_str: str, type_info: str | dict | None) -> str`
Transform JSON with `__typename` injection using manual type mapping.

```python
# Simple string typename
result = fraiseql_rs.transform_json_with_typename(input_json, "User")

# Type map for nested structures
type_map = {
    "$": "User",
    "posts": "Post",
    "posts.comments": "Comment"
}
result = fraiseql_rs.transform_json_with_typename(input_json, type_map)
```

#### `transform_with_schema(json_str: str, root_type: str, schema: dict) -> str`
Transform JSON using a GraphQL-like schema definition. **Best option for complex schemas.**

```python
schema = {
    "User": {
        "fields": {
            "id": "Int",
            "name": "String",
            "posts": "[Post]"  # Automatic array type detection
        }
    }
}
result = fraiseql_rs.transform_with_schema(input_json, "User", schema)
```

### SchemaRegistry Class

Reusable schema for optimal performance when transforming multiple records.

```python
# Create registry and register types
registry = fraiseql_rs.SchemaRegistry()
registry.register_type("User", {
    "fields": {
        "id": "Int",
        "name": "String",
        "posts": "[Post]"
    }
})
registry.register_type("Post", {
    "fields": {
        "id": "Int",
        "title": "String"
    }
})

# Transform efficiently (schema parsed once)
for record in records:
    result = registry.transform(record, "User")
```

## Schema Definition

### Field Types

**Scalars**: Built-in GraphQL types
- `"Int"`, `"String"`, `"Boolean"`, `"Float"`, `"ID"`

**Objects**: Custom types
- `"User"`, `"Post"`, `"Profile"`

**Arrays**: Array notation with brackets
- `"[Post]"` - Array of Post objects
- `"[Comment]"` - Array of Comment objects

### Example Schema

```python
schema = {
    "User": {
        "fields": {
            # Scalars
            "id": "Int",
            "name": "String",
            "is_active": "Boolean",

            # Nested object
            "profile": "Profile",

            # Arrays
            "posts": "[Post]",
            "friends": "[User]"
        }
    },
    "Profile": {
        "fields": {
            "bio": "String",
            "avatar_url": "String"
        }
    },
    "Post": {
        "fields": {
            "id": "Int",
            "title": "String",
            "comments": "[Comment]"  # Nested arrays
        }
    },
    "Comment": {
        "fields": {
            "id": "Int",
            "text": "String",
            "author": "User"  # Circular references supported
        }
    }
}
```

## Performance

### Typical Response Times

| Operation | Time | Speedup vs Python |
|-----------|------|-------------------|
| Simple object (10 fields) | 0.006ms | ~9x faster |
| Medium object (42 fields) | 0.016ms | ~8x faster |
| Nested (User + posts) | 0.094ms | ~10x faster |
| Large (100 fields) | 0.453ms | ~5x faster |

### Performance Characteristics

- **Zero-copy JSON parsing**: Minimal allocations
- **Move semantics**: No value cloning
- **Single-pass transformation**: No redundant iterations
- **O(1) type lookups**: HashMap-based schema
- **GIL-free**: True parallel execution

## Use Cases

### 1. GraphQL API Responses

```python
# Transform database results to GraphQL responses
db_result = await db.execute(query)
json_str = db_result.scalar_one()  # JSONB from PostgreSQL

result = registry.transform(json_str, "User")
return JSONResponse(content=result)
```

### 2. Batch Processing

```python
# Process thousands of records efficiently
for record in records:
    transformed = registry.transform(record.data, "User")
    await send_to_client(transformed)
```

### 3. Real-time Transformations

```python
# WebSocket streaming with minimal latency
async for message in websocket:
    result = fraiseql_rs.transform_with_schema(message, "Event", schema)
    await websocket.send(result)
```

## Integration with FraiseQL

```python
from fraiseql import GraphQLType, Field
import fraiseql_rs

class User(GraphQLType):
    id: int
    name: str
    posts: list["Post"] = Field(default_factory=list)

class Post(GraphQLType):
    id: int
    title: str

# Build schema at startup
def build_schema(*types):
    schema = {}
    for type_cls in types:
        fields = {}
        for name, field in type_cls.__fields__.items():
            # Map Python types to schema types
            if field.type == int:
                fields[name] = "Int"
            elif field.type == str:
                fields[name] = "String"
            elif hasattr(field.type, "__origin__"):  # list[T]
                inner = field.type.__args__[0]
                fields[name] = f"[{inner.__name__}]"
        schema[type_cls.__name__] = {"fields": fields}
    return schema

# Create registry once
schema = build_schema(User, Post)
registry = fraiseql_rs.SchemaRegistry()
for type_name, type_def in schema.items():
    registry.register_type(type_name, type_def)

# Use in resolvers
async def resolve_user(info, user_id: int):
    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    json_str = result.scalar_one()
    return registry.transform(json_str, "User")
```

## Choosing the Right Function

| Use Case | Function | Why |
|----------|----------|-----|
| No type info needed | `transform_json()` | Fastest, simple camelCase only |
| Simple types | `transform_json_with_typename()` | Manual control, flexible |
| Complex schemas | `transform_with_schema()` | Clean API, automatic arrays |
| Repeated transformations | `SchemaRegistry` | Best performance, parse once |

## Development

### Building

```bash
# Development build
maturin develop

# Release build
maturin build --release
```

### Testing

```bash
# Run Python integration tests
pytest tests/integration/rust/

# Run Rust unit tests
cd fraiseql_rs
cargo test
```

### Linting

```bash
cd fraiseql_rs
cargo clippy -- -D warnings
```

## Architecture

### Module Structure

```
fraiseql_rs/
├── src/
│   ├── lib.rs                   # Python bindings
│   ├── camel_case.rs            # String conversion
│   ├── json_transform.rs        # JSON parsing
│   ├── typename_injection.rs    # __typename logic
│   └── schema_registry.rs       # Schema-aware transformation
└── Cargo.toml
```

### Design Principles

1. **Zero-copy where possible**: Minimize allocations
2. **Single-pass transformations**: No redundant iterations
3. **Type-safe**: Rust's type system prevents errors
4. **Ergonomic API**: Pythonic interface with Rust performance
5. **Composable**: Functions build on each other

## Requirements

- Python 3.8+
- Rust 1.70+
- PyO3 0.25+
- serde_json 1.0+

## License

See LICENSE file for details.

## Contributing

Contributions welcome! Please ensure:
- All tests pass (`pytest tests/integration/rust/`)
- Code is formatted (`cargo fmt`)
- Linting passes (`cargo clippy`)
- Documentation is updated

## Credits

Built with [PyO3](https://pyo3.rs/) for Python-Rust interop and [serde_json](https://github.com/serde-rs/json) for JSON parsing.
