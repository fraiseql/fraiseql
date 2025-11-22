# fraiseql-rs API Reference

Complete API documentation for the fraiseql-rs Python extension module.

## Table of Contents

- [Functions](#functions)
  - [to_camel_case](#to_camel_case)
  - [transform_keys](#transform_keys)
  - [transform_json](#transform_json)
  - [transform_json_with_typename](#transform_json_with_typename)
  - [transform_with_schema](#transform_with_schema)
- [Classes](#classes)
  - [SchemaRegistry](#schemaregistry)
- [Type Definitions](#type-definitions)
- [Error Handling](#error-handling)
- [Performance Tips](#performance-tips)

---

## Functions

### `to_camel_case`

Convert a single snake_case string to camelCase.

**Signature:**
```python
def to_camel_case(s: str) -> str
```

**Parameters:**
- `s` (str): The snake_case string to convert

**Returns:**
- str: The camelCase string

**Examples:**
```python
>>> fraiseql_rs.to_camel_case("user_name")
"userName"

>>> fraiseql_rs.to_camel_case("email_address")
"emailAddress"

>>> fraiseql_rs.to_camel_case("billing_address_line_1")
"billingAddressLine1"
```

**Performance:**
- **Time**: ~0.01-0.05ms per string
- **Speedup**: 20-100x vs Python

**Notes:**
- Leading underscores are preserved: `"_private"` → `"_private"`
- Multiple consecutive underscores are treated as single: `"user__name"` → `"userName"`
- Numbers are preserved: `"address_line_1"` → `"addressLine1"`

---

### `transform_keys`

Transform dictionary keys from snake_case to camelCase.

**Signature:**
```python
def transform_keys(obj: dict, recursive: bool = False) -> dict
```

**Parameters:**
- `obj` (dict): Dictionary with snake_case keys
- `recursive` (bool, optional): If True, recursively transform nested dicts and lists. Default: False

**Returns:**
- dict: New dictionary with camelCase keys

**Examples:**
```python
>>> data = {"user_id": 1, "user_name": "John"}
>>> fraiseql_rs.transform_keys(data)
{"userId": 1, "userName": "John"}

>>> nested = {
...     "user_id": 1,
...     "user_profile": {
...         "first_name": "John"
...     }
... }
>>> fraiseql_rs.transform_keys(nested, recursive=True)
{"userId": 1, "userProfile": {"firstName": "John"}}
```

**Performance:**
- **Time**: ~0.2-0.5ms for 20 fields
- **Speedup**: 10-50x vs Python

**Use Cases:**
- When you already have Python dicts in memory
- Simple, one-level transformations
- When you need to preserve Python dict types

---

### `transform_json`

Transform JSON string with camelCase conversion (no typename injection).

**Signature:**
```python
def transform_json(json_str: str) -> str
```

**Parameters:**
- `json_str` (str): JSON string with snake_case keys

**Returns:**
- str: Transformed JSON string with camelCase keys

**Raises:**
- `ValueError`: If json_str is not valid JSON

**Examples:**
```python
>>> input_json = '{"user_id": 1, "user_posts": [{"post_id": 1}]}'
>>> fraiseql_rs.transform_json(input_json)
'{"userId":1,"userPosts":[{"postId":1}]}'
```

**Performance:**
- **Time**: ~0.1-0.2ms for simple objects, ~0.5-1ms for complex
- **Speedup**: 10-50x vs Python
- **Fastest option** when no typename is needed

**Use Cases:**
- Pure camelCase transformation
- No GraphQL type information needed
- Maximum performance for simple transformations

**Performance Characteristics:**
- Zero-copy JSON parsing
- Move semantics (no value cloning)
- Single-pass transformation
- GIL-free execution

---

### `transform_json_with_typename`

Transform JSON with `__typename` injection using manual type mapping.

**Signature:**
```python
def transform_json_with_typename(
    json_str: str,
    type_info: str | dict | None
) -> str
```

**Parameters:**
- `json_str` (str): JSON string with snake_case keys
- `type_info` (str | dict | None): Type information
  - `str`: Simple typename for root object (e.g., `"User"`)
  - `dict`: Type map for nested objects (e.g., `{"$": "User", "posts": "Post"}`)
  - `None`: No typename injection (behaves like `transform_json`)

**Returns:**
- str: Transformed JSON string with camelCase keys and `__typename` fields

**Raises:**
- `ValueError`: If json_str is not valid JSON or type_info is invalid

**Examples:**

**Simple string typename:**
```python
>>> input_json = '{"user_id": 1, "user_name": "John"}'
>>> fraiseql_rs.transform_json_with_typename(input_json, "User")
'{"__typename":"User","userId":1,"userName":"John"}'
```

**Type map for nested structures:**
```python
>>> type_map = {
...     "$": "User",
...     "posts": "Post",
...     "posts.comments": "Comment"
... }
>>> result = fraiseql_rs.transform_json_with_typename(input_json, type_map)
```

**No typename (None):**
```python
>>> fraiseql_rs.transform_json_with_typename(input_json, None)
'{"userId":1,"userName":"John"}'  # Same as transform_json
```

**Type Map Format:**
- `"$"` or `""`: Root type
- `"field_name"`: Type for field or array elements
- `"parent.child"`: Nested path for deeply nested structures

**Performance:**
- **Time**: ~0.1-0.3ms for simple, ~1.5-3ms for complex nested
- **Overhead**: ~10-20% vs `transform_json`
- Type lookup is O(1) average (HashMap)

**Use Cases:**
- Simple schemas (< 5 types)
- Dynamic type resolution
- One-off transformations
- Fine-grained control over type mapping

---

### `transform_with_schema`

Transform JSON using a GraphQL-like schema definition with automatic type detection.

**Signature:**
```python
def transform_with_schema(
    json_str: str,
    root_type: str,
    schema: dict
) -> str
```

**Parameters:**
- `json_str` (str): JSON string with snake_case keys
- `root_type` (str): Root type name from schema (e.g., `"User"`)
- `schema` (dict): Schema definition dict mapping type names to field definitions

**Returns:**
- str: Transformed JSON string with camelCase keys and `__typename` fields

**Raises:**
- `ValueError`: If json_str is not valid JSON or schema is invalid

**Schema Format:**
```python
schema = {
    "TypeName": {
        "fields": {
            "field_name": "FieldType",
            "array_field": "[ElementType]",
            "nested_field": "NestedType"
        }
    }
}
```

**Field Types:**
- **Scalars**: `"Int"`, `"String"`, `"Boolean"`, `"Float"`, `"ID"`
- **Objects**: `"User"`, `"Post"`, `"Profile"` (custom type names)
- **Arrays**: `"[Post]"`, `"[Comment]"` (bracket notation)

**Examples:**

**Simple schema:**
```python
>>> schema = {
...     "User": {
...         "fields": {
...             "id": "Int",
...             "name": "String",
...             "posts": "[Post]"
...         }
...     },
...     "Post": {
...         "fields": {
...             "id": "Int",
...             "title": "String"
...         }
...     }
... }
>>> result = fraiseql_rs.transform_with_schema(input_json, "User", schema)
```

**Complex nested schema:**
```python
>>> schema = {
...     "User": {
...         "fields": {
...             "id": "Int",
...             "posts": "[Post]"
...         }
...     },
...     "Post": {
...         "fields": {
...             "id": "Int",
...             "comments": "[Comment]"
...         }
...     },
...     "Comment": {
...         "fields": {
...             "id": "Int",
...             "author": "User"  # Circular reference
...         }
...     }
... }
```

**Performance:**
- **Time**: Same as `transform_json_with_typename` (~1.5-3ms for complex)
- **Schema parsing**: ~0.05-0.2ms (one-time cost)
- Use `SchemaRegistry` to amortize parsing cost

**Use Cases:**
- **Complex schemas** (5+ types)
- **Static schemas** (known upfront)
- **Clean API** (no manual type maps)
- **Production use** with FraiseQL

**Advantages over `transform_json_with_typename`:**
- Automatic array detection with `[Type]` notation
- Self-documenting schema
- Easier to maintain
- No manual path notation

---

## Classes

### `SchemaRegistry`

Reusable schema registry for optimal performance when transforming multiple records.

**Constructor:**
```python
registry = fraiseql_rs.SchemaRegistry()
```

**Methods:**

#### `register_type`

Register a type in the schema.

**Signature:**
```python
def register_type(self, type_name: str, type_def: dict) -> None
```

**Parameters:**
- `type_name` (str): Name of the type (e.g., `"User"`)
- `type_def` (dict): Type definition dict with `"fields"` key

**Example:**
```python
>>> registry = fraiseql_rs.SchemaRegistry()
>>> registry.register_type("User", {
...     "fields": {
...         "id": "Int",
...         "name": "String",
...         "posts": "[Post]"
...     }
... })
```

#### `transform`

Transform JSON using the registered schema.

**Signature:**
```python
def transform(self, json_str: str, root_type: str) -> str
```

**Parameters:**
- `json_str` (str): JSON string to transform
- `root_type` (str): Root type name (e.g., `"User"`)

**Returns:**
- str: Transformed JSON string with camelCase keys and `__typename` fields

**Raises:**
- `ValueError`: If json_str is not valid JSON

**Example:**
```python
>>> registry = fraiseql_rs.SchemaRegistry()
>>> registry.register_type("User", user_def)
>>> registry.register_type("Post", post_def)
>>>
>>> for record in records:
...     result = registry.transform(record, "User")
```

**Performance Advantage:**

```python
# Without SchemaRegistry (parse schema every time)
for record in 1000 records:
    result = fraiseql_rs.transform_with_schema(record, "User", schema)
# Total: 1000 × (0.1ms parse + 1ms transform) = 1100ms

# With SchemaRegistry (parse schema once)
registry = fraiseql_rs.SchemaRegistry()
registry.register_type("User", user_def)
registry.register_type("Post", post_def)

for record in 1000 records:
    result = registry.transform(record, "User")
# Total: 0.1ms parse + 1000 × 1ms transform = 1000ms
# Saves ~100ms (10% improvement)
```

**Use Cases:**
- **Batch processing**: Transform many records
- **Long-running services**: Parse schema once at startup
- **Repeated transformations**: Same schema, different data
- **Best performance**: Minimum overhead

---

## Type Definitions

### Scalar Types

Built-in GraphQL scalar types:

| Type | Description | Example |
|------|-------------|---------|
| `"Int"` | Integer number | `42` |
| `"String"` | Text string | `"hello"` |
| `"Boolean"` | True/false value | `true` |
| `"Float"` | Floating point number | `3.14` |
| `"ID"` | Unique identifier | `"user-123"` |

### Object Types

Custom types defined in your schema:

```python
"User", "Post", "Profile", "Comment"
```

### Array Types

Arrays of objects using bracket notation:

```python
"[Post]"      # Array of Post objects
"[Comment]"   # Array of Comment objects
"[User]"      # Array of User objects
```

**Nesting:**
Arrays can be deeply nested:

```python
schema = {
    "User": {"fields": {"posts": "[Post]"}},
    "Post": {"fields": {"comments": "[Comment]"}},
    "Comment": {"fields": {"replies": "[Comment]"}}
}
```

---

## Error Handling

### ValueError

Raised when JSON parsing fails or input is invalid.

**Common Causes:**
- Invalid JSON syntax
- Malformed type_info parameter
- Invalid schema definition

**Example:**
```python
try:
    result = fraiseql_rs.transform_json("not valid json")
except ValueError as e:
    print(f"JSON error: {e}")
    # Output: JSON error: Invalid JSON: expected ident at line 1 column 2
```

**Best Practices:**
- Always validate JSON before transformation
- Use try-except blocks for error handling
- Log errors for debugging
- Return meaningful error messages to clients

---

## Performance Tips

### 1. Choose the Right Function

| Scenario | Best Choice | Reason |
|----------|-------------|--------|
| No typename needed | `transform_json()` | Fastest |
| Simple typename | `transform_json_with_typename()` | Flexible |
| Complex schema | `transform_with_schema()` | Clean API |
| Repeated transforms | `SchemaRegistry` | Parse once |

### 2. Use SchemaRegistry for Batch Processing

```python
# ❌ Slow: Parse schema every time
for record in records:
    result = fraiseql_rs.transform_with_schema(record, "User", schema)

# ✅ Fast: Parse schema once
registry = fraiseql_rs.SchemaRegistry()
registry.register_type("User", user_def)
for record in records:
    result = registry.transform(record, "User")
```

### 3. Reuse Registry Across Requests

```python
# At app startup
schema_registry = fraiseql_rs.SchemaRegistry()
for type_name, type_def in schema.items():
    schema_registry.register_type(type_name, type_def)

# In request handlers
async def handle_request(data):
    result = schema_registry.transform(data, "User")
    return result
```

### 4. Profile Your Use Case

```python
import time

# Measure transformation time
start = time.perf_counter()
result = fraiseql_rs.transform_with_schema(data, "User", schema)
duration = (time.perf_counter() - start) * 1000
print(f"Transformation took {duration:.2f}ms")
```

### 5. Optimize Schema Definitions

```python
# ✅ Good: Minimal schema
schema = {
    "User": {
        "fields": {
            "id": "Int",
            "name": "String",
            "posts": "[Post]"
        }
    }
}

# ❌ Avoid: Redundant fields not in your data
# Only include fields you actually use
```

### 6. Parallel Processing

```python
import asyncio

# fraiseql-rs is GIL-free, so you can use multiprocessing
from multiprocessing import Pool

def transform_record(record):
    return registry.transform(record, "User")

# Process records in parallel
with Pool(processes=4) as pool:
    results = pool.map(transform_record, records)
```

---

## Examples

### Complete FraiseQL Integration

```python
from fraiseql import GraphQLType, Field
import fraiseql_rs

# Define GraphQL types
class User(GraphQLType):
    id: int
    name: str
    email: str
    posts: list["Post"] = Field(default_factory=list)

class Post(GraphQLType):
    id: int
    title: str
    content: str
    comments: list["Comment"] = Field(default_factory=list)

class Comment(GraphQLType):
    id: int
    text: str
    author: "User"

# Build schema from types
def build_schema(*types):
    schema = {}
    for type_cls in types:
        fields = {}
        for name, field in type_cls.__fields__.items():
            if field.type == int:
                fields[name] = "Int"
            elif field.type == str:
                fields[name] = "String"
            elif hasattr(field.type, "__origin__"):
                inner = field.type.__args__[0]
                fields[name] = f"[{inner.__name__}]"
            else:
                fields[name] = field.type.__name__
        schema[type_cls.__name__] = {"fields": fields}
    return schema

# Create registry at startup
schema = build_schema(User, Post, Comment)
registry = fraiseql_rs.SchemaRegistry()
for type_name, type_def in schema.items():
    registry.register_type(type_name, type_def)

# Use in resolvers
async def resolve_user(info, user_id: int):
    # Query database
    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    json_str = result.scalar_one()

    # Transform with fraiseql-rs
    return registry.transform(json_str, "User")
```

### Streaming Transformations

```python
import asyncio
import fraiseql_rs

registry = fraiseql_rs.SchemaRegistry()
registry.register_type("Event", event_def)

async def stream_events(websocket):
    async for message in websocket:
        # Transform in real-time
        transformed = registry.transform(message, "Event")
        await websocket.send(transformed)
```

### Batch Processing

```python
import fraiseql_rs

registry = fraiseql_rs.SchemaRegistry()
registry.register_type("User", user_def)

# Process 10,000 records efficiently
for batch in batches(records, size=100):
    results = [
        registry.transform(record, "User")
        for record in batch
    ]
    await process_batch(results)
```

---

## Changelog

See [CHANGELOG.md](../../CHANGELOG.md) for version history and breaking changes.

## Contributing

See [README.md](../../README.md#contributing) for contribution guidelines.
