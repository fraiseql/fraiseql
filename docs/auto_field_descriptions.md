# Automatic Field Descriptions

FraiseQL now automatically extracts field descriptions from multiple sources to enhance your GraphQL schema documentation without requiring explicit configuration.

## Overview

The automatic field description feature extracts documentation from:

1. **Class docstring field documentation** (lowest priority)
2. **Annotated type hints** (medium priority)
3. **Inline comments in source code** (highest priority - not available for dynamically created classes)

This provides zero-configuration documentation that appears in Apollo Studio, GraphQL Playground, and schema introspection.

## Supported Sources

### 1. Docstring Field Documentation

Document fields in your class docstring using `Fields:`, `Attributes:`, or `Args:` sections:

```python
@fraiseql.fraise_type
@dataclass
class User:
    """User account with authentication capabilities.

    Fields:
        id: Unique user identifier
        username: Username for authentication
        email: User's email address for communication
        is_active: Whether the account is currently active
    """
    id: UUID
    username: str
    email: str
    is_active: bool = True
```

### 2. Annotated Type Hints

Use Python's `Annotated` type hints to include field descriptions:

```python
from typing import Annotated

@fraiseql.fraise_type
@dataclass
class Product:
    """Product catalog item."""
    id: Annotated[UUID, "Product identifier"]
    name: Annotated[str, "Product display name"]
    price: Annotated[float, "Price in USD"]
    stock_count: int  # No description
```

### 3. Inline Comments (Source Code Only)

For classes defined in source files (not dynamically), inline comments are automatically extracted:

```python
@fraiseql.fraise_type
@dataclass
class Order:
    """Customer order information."""
    id: UUID  # Order identifier
    customer_id: UUID  # Customer who placed the order
    total_amount: float  # Total order value in USD
    status: str = "pending"  # Current order status
```

**Note:** Inline comments only work for classes defined in source files, not for dynamically created classes.

## Priority System

When multiple sources provide descriptions for the same field, they are applied in priority order:

1. **Inline comments** (highest) - overrides all other sources
2. **Annotated type hints** (medium) - overrides docstring descriptions
3. **Docstring sections** (lowest) - used when no other source available

### Example with Multiple Sources:

```python
@fraiseql.fraise_type
@dataclass
class MixedExample:
    """Example with multiple description sources.

    Fields:
        field1: Description from docstring
        field2: Docstring description (will be overridden)
    """
    field1: str  # This inline comment takes priority
    field2: Annotated[str, "Annotation description takes priority"]
    field3: str  # Only inline comment, no conflict
```

Result:
- `field1`: "This inline comment takes priority"
- `field2`: "Annotation description takes priority"
- `field3`: "Only inline comment, no conflict"

## Input Types

Automatic descriptions work for input types using the `Args:` section:

```python
@fraiseql.fraise_input
@dataclass
class CreateUserInput:
    """Input for creating a new user account.

    Args:
        username: Desired username (must be unique)
        email: User's email address
        password: Account password (will be hashed)
    """
    username: str
    email: str
    password: str
```

## Backward Compatibility

Existing explicit field descriptions are preserved:

```python
@fraiseql.fraise_type
@dataclass
class BackwardCompatible:
    """Type with mixed explicit and automatic descriptions.

    Fields:
        name: Auto description from docstring
    """
    id: UUID  # Auto description from comment
    name: str = fraiseql.fraise_field(description="Explicit description preserved")
    email: str  # Auto description from comment
```

Result:
- `id`: "Auto description from comment"
- `name`: "Explicit description preserved" (not overridden)
- `email`: "Auto description from comment"

## Best Practices

### 1. Choose One Primary Method

While mixing is supported, consistency improves maintainability:

```python
# ✅ Good: Consistent docstring approach
@fraiseql.fraise_type
@dataclass
class User:
    """User model.

    Fields:
        id: User identifier
        name: Display name
        email: Contact email
    """
    id: UUID
    name: str
    email: str
```

### 2. Use Meaningful Descriptions

Provide context beyond the field name:

```python
# ❌ Poor: Redundant descriptions
Fields:
    name: User name
    email: User email

# ✅ Good: Informative descriptions
Fields:
    name: Full display name for UI presentation
    email: Primary contact email for notifications
```

### 3. Document Complex Types

Explain relationships and data structure:

```python
@fraiseql.fraise_type
@dataclass
class Order:
    """Customer order with line items.

    Fields:
        id: Unique order identifier
        customer_id: Foreign key to customer table
        line_items: Products and quantities in this order
        total_amount: Calculated sum of all line items including tax
        created_at: Order placement timestamp in UTC
    """
    id: UUID
    customer_id: UUID
    line_items: list[OrderItem]
    total_amount: float
    created_at: datetime
```

## GraphQL Schema Output

All automatic descriptions appear in the generated GraphQL schema:

```graphql
"""User account with authentication capabilities."""
type User {
  """Unique user identifier"""
  id: ID!

  """Username for authentication"""
  username: String!

  """User's email address for communication"""
  email: String!

  """Whether the account is currently active"""
  isActive: Boolean!
}
```

## Apollo Studio Integration

Automatic descriptions enhance the developer experience in Apollo Studio:

- **Type browser**: Shows field descriptions in the schema explorer
- **Query builder**: Displays field descriptions as tooltips
- **Documentation**: Auto-generated docs include all field information
- **IntelliSense**: IDE integration shows descriptions during development

## Migration Guide

### From Manual Documentation

If you have existing manual field descriptions:

```python
# Before: Manual descriptions
class User:
    id: UUID = fraiseql.fraise_field(description="User ID")
    name: str = fraiseql.fraise_field(description="Display name")

# After: Automatic descriptions
class User:
    """User account.

    Fields:
        id: User ID
        name: Display name
    """
    id: UUID
    name: str
```

### Adding to Existing Types

For existing types without descriptions:

```python
# Step 1: Add class docstring with Fields section
@fraiseql.fraise_type
@dataclass
class ExistingType:
    """Add this docstring.

    Fields:
        field1: Description for field1
        field2: Description for field2
    """
    field1: str
    field2: int
```

## Limitations

1. **Inline comments**: Only work for source file classes, not dynamic classes
2. **Annotation support**: Depends on Python version and typing system
3. **Docstring parsing**: Requires specific format (`Fields:`, `Attributes:`, `Args:`)
4. **Source availability**: Some deployment environments may not have source access

## Troubleshooting

### No Descriptions Appearing

1. **Check docstring format**:
   ```python
   # ❌ Wrong format
   """
   id - User identifier
   """

   # ✅ Correct format
   """
   Fields:
       id: User identifier
   """
   ```

2. **Verify field names match**:
   ```python
   # Field name in docstring must exactly match Python field name
   Fields:
       user_id: Description  # Must match field name exactly
   ```

3. **Check explicit descriptions**:
   Explicit `fraise_field(description=...)` takes precedence over automatic extraction.

### Source Code Not Available

For dynamically created classes or restricted environments:

```python
# Use docstring method instead of inline comments
@fraiseql.fraise_type
@dataclass
class DynamicClass:
    """Use docstring method for dynamic classes.

    Fields:
        id: Field description here instead of inline comment
    """
    id: UUID
```

## Examples

See `examples/auto_field_descriptions.py` for a complete working example demonstrating all features of automatic field description extraction.

---

*This feature enhances the existing v0.9.1 automatic docstring extraction by adding field-level description support, providing comprehensive zero-configuration GraphQL schema documentation.*
