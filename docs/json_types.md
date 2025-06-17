# JSON and Dictionary Types in FraiseQL

FraiseQL provides full support for JSON data types, allowing you to work with semi-structured data in your GraphQL API.

## Supported Type Annotations

You can use any of the following type annotations for JSON fields:

### 1. Using `dict` types:
```python
from typing import Any
from fraiseql import fraise_type

@fraise_type
class Configuration:
    # Plain dict - accepts any JSON object
    settings: dict

    # Dict with type parameters - also mapped to JSON
    metadata: dict[str, Any]

    # Optional JSON field
    details: dict[str, Any] | None
```

### 2. Using the `JSON` type alias:
```python
from fraiseql.types import JSON
from fraiseql import fraise_type

@fraise_type
class Product:
    # Using the JSON type alias
    attributes: JSON

    # Optional JSON
    custom_data: JSON | None
```

## What Can Be Stored

JSON fields can store any valid JSON data:
- Objects (dictionaries)
- Arrays (lists)
- Strings
- Numbers (integers and floats)
- Booleans
- null

## Example Usage

```python
from typing import Any
from fraiseql import fraise_type

@fraise_type
class Error:
    message: str
    code: str
    details: dict[str, Any] | None = None

# In your query resolver:
async def get_error(info) -> Error:
    return Error(
        message="Validation failed",
        code="VALIDATION_ERROR",
        details={
            "fields": {
                "email": ["Invalid email format"],
                "age": ["Must be 18 or older"]
            },
            "request_id": "abc123"
        }
    )
```

## GraphQL Schema

All JSON types are represented as the `JSON` scalar in the GraphQL schema:

```graphql
type Error {
  message: String!
  code: String!
  details: JSON
}

scalar JSON
```

## Querying JSON Fields

JSON fields are returned as-is in GraphQL queries:

```graphql
query {
  get_error {
    message
    code
    details  # Returns the full JSON structure
  }
}
```

Response:
```json
{
  "data": {
    "get_error": {
      "message": "Validation failed",
      "code": "VALIDATION_ERROR",
      "details": {
        "fields": {
          "email": ["Invalid email format"],
          "age": ["Must be 18 or older"]
        },
        "request_id": "abc123"
      }
    }
  }
}
```

## Database Storage

JSON fields are stored as PostgreSQL `JSONB` columns, providing:
- Efficient storage and indexing
- Support for JSON operators and functions
- Automatic validation of JSON structure

## Migration from Strawberry

If you're migrating from Strawberry GraphQL and had issues with JSON types:

**Before (Strawberry):**
```python
# This might not have worked
class Error:
    details: dict[str, Any]  # Unsupported
```

**After (FraiseQL):**
```python
from fraiseql import fraise_type

@fraise_type
class Error:
    details: dict[str, Any]  # Fully supported!
```

## Best Practices

1. **Use type hints**: While `dict` works, `dict[str, Any]` is more explicit about accepting any JSON
2. **Consider validation**: For structured data, consider using typed fields instead of JSON
3. **Document structure**: Add docstrings explaining the expected JSON structure
4. **Use defaults**: Provide sensible defaults for optional JSON fields

```python
@fraise_type
class Settings:
    """User settings configuration."""

    # Well-documented JSON field
    preferences: dict[str, Any] = None
    """
    User preferences as JSON:
    {
        "theme": "dark" | "light",
        "notifications": {
            "email": bool,
            "push": bool
        }
    }
    """
```
