# Types Module Structure

**Location**: `src/fraiseql/types/`
**Purpose**: Type system, decorators, and 40+ custom scalar types
**Stability**: Stable - scalars rarely change
**Test Coverage**: 100+ scalar type tests in `tests/unit/types/scalars/`

## Overview

The `types` module defines the GraphQL type system including decorators, custom scalars, and type utilities. It provides the foundation for schema definition.

## Module Organization

### `fraise_type.py`
**Responsibility**: `@fraise_type` decorator for defining GraphQL types
**Public API**:
- `@fraise_type`: Type decorator
- `FraiseType`: Base type class

**Example**:
```python
from fraiseql.types import fraise_type

@fraise_type
class User:
    """A user in the system."""
    id: ID
    name: str
    email: str | None = None
```

**Depends On**: Type validation, decorators
**Used By**: Schema building, HTTP servers

---

### `fraise_input.py`
**Responsibility**: `@fraise_input` decorator for input types
**Public API**:
- `@fraise_input`: Input type decorator
- `InputType`: Base input class

**Used For**: Query arguments, mutation inputs
**Validates**: Required fields, type checking

---

### `fraise_interface.py`
**Responsibility**: `@fraise_interface` decorator for GraphQL interfaces
**Public API**:
- `@fraise_interface`: Interface decorator
- `InterfaceType`: Base interface class

**Used For**: Shared type fields, polymorphism

---

### `enum.py`
**Responsibility**: GraphQL enum type support
**Public API**:
- `@fraise_enum`: Enum decorator
- Enum validation

**Example**:
```python
from fraiseql.types import fraise_enum

@fraise_enum
class Status:
    ACTIVE = "active"
    INACTIVE = "inactive"
```

---

### `context.py`
**Responsibility**: GraphQL context object
**Public API**:
- `Context`: Request context class
- Context creation and management

**Contains**: User info, request data, app state

**Used By**: Every resolver function

---

### `definitions.py`
**Responsibility**: Special type markers and sentinels
**Public API**:
- `UNSET`: Sentinel value for unset fields
- Type definition helpers
- Marker types

---

### `common.py`
**Responsibility**: Common type definitions
**Public API**:
- `MutationResultBase`: Base for mutation results
- `MutationError`: Error in mutation
- Standard result types

---

### `errors.py`
**Responsibility**: GraphQL error types
**Public API**:
- `GraphQLError`: GraphQL-specific error
- `ValidationError`: Validation error
- `AuthorizationError`: Auth error

---

### `scalars/` (40+ Custom Types)
**Purpose**: Custom scalar type definitions
**Organization**: Organized by category

#### Standard Scalars
```
scalars/standard/
├── date.py                 # Date type
├── datetime.py             # DateTime type
├── uuid.py                 # UUID type
├── json.py                 # JSON type
└── time.py                 # Time type
```

**Purpose**: Common types across all applications

#### Network Scalars
```
scalars/network/
├── ip_address.py           # IPv4/IPv6
├── port.py                 # Network port
├── cidr.py                 # CIDR notation
└── mac_address.py          # MAC address
```

**Purpose**: Network-related types

#### Financial Scalars
```
scalars/financial/
├── money.py                # Currency amount
├── currency_code.py        # ISO currency code
└── percentage.py           # Percentage values
```

**Purpose**: Financial/currency types

#### Contact Scalars
```
scalars/contact/
├── email_address.py        # Email validation
├── phone_number.py         # Phone number
├── url.py                  # URL type
└── slug.py                 # URL slug
```

**Purpose**: Contact information types

#### Geographic Scalars
```
scalars/geographic/
├── coordinates.py          # Lat/Long
├── geojson.py              # GeoJSON type
└── country_code.py         # ISO country code
```

**Purpose**: Location/mapping types

#### Project-Specific
```
scalars/[custom]/
├── my_type.py              # Custom types
```

**Purpose**: Application-specific types

---

## Scalar Type Template

Each scalar follows this pattern:

```python
"""Email address scalar type.

Provides serialization and validation for email addresses.

Example:
    >>> EmailAddress.serialize("user@example.com")
    "user@example.com"
"""

from typing import Any

class EmailAddressScalar:
    """Email address GraphQL scalar."""

    @staticmethod
    def serialize(value: Any) -> str:
        """Convert Python value to GraphQL representation."""
        if isinstance(value, str):
            return value
        raise TypeError(f"Invalid email: {value}")

    @staticmethod
    def parse_value(value: Any) -> str:
        """Parse client-provided value."""
        if not isinstance(value, str):
            raise TypeError(f"Email must be string, got {type(value)}")
        if "@" not in value:
            raise ValueError(f"Invalid email format: {value}")
        return value

    @staticmethod
    def parse_literal(ast) -> str:
        """Parse GraphQL literal value."""
        if hasattr(ast, "value"):
            return EmailAddressScalar.parse_value(ast.value)
        raise ValueError(f"Invalid email literal: {ast}")
```

---

## Dependencies

### Internal Dependencies
```
fraise_type.py
├── type_validation
├── decorators
└── common.py

fraise_input.py
├── type_validation
├── decorators
└── common.py

scalars/
├── __init__.py             # Central exports
└── [scalar implementations]
```

### External Dependencies
- `graphql-core`: GraphQL type system
- `pydantic`: Validation

---

## Adding New Scalar Types

### Step 1: Determine Category
- Standard (dates, JSON)
- Network (IP, port)
- Financial (money, currency)
- Contact (email, phone)
- Geographic (coordinates)
- Custom (application-specific)

### Step 2: Create File
```
scalars/[category]/my_type.py
```

### Step 3: Implement Scalar
```python
"""My custom type scalar."""

class MyTypeScalar:
    """My custom type GraphQL scalar."""

    @staticmethod
    def serialize(value):
        """Convert to GraphQL representation."""
        ...

    @staticmethod
    def parse_value(value):
        """Parse client value."""
        ...

    @staticmethod
    def parse_literal(ast):
        """Parse GraphQL literal."""
        ...
```

### Step 4: Add Tests
```
tests/unit/types/scalars/test_my_type.py
```

### Step 5: Export
Update `scalars/__init__.py`:
```python
from .[category].my_type import MyTypeScalar

__all__ = [..., "MyTypeScalar"]
```

### Step 6: Document
Add example to scalar docstring and API documentation

---

## Guidelines for New Types

1. **One responsibility**: Each type/decorator does one thing
2. **Consistent naming**: `Fraise` prefix for framework decorators
3. **Validation**: All types validate input
4. **Error messages**: Clear, actionable error messages
5. **Documentation**: Docstrings with examples
6. **Tests**: Happy path + error cases

---

## Common Questions

**Q: How do I create a custom scalar type?**
A: Follow "Adding New Scalar Types" above, use template from `scalars/standard/` as reference.

**Q: How do I use a scalar in my schema?**
A: Register in `@fraise_type` decorator and reference by name:
```python
@fraise_type
class User:
    email: EmailAddress
    created_at: DateTime
```

**Q: Can I extend existing scalars?**
A: Yes, subclass and override `serialize()`, `parse_value()`, or `parse_literal()`.

**Q: Where do validation rules go?**
A: Add to `parse_value()` and `parse_literal()` methods with clear error messages.

---

## Refactoring Roadmap

### v2.0 (Current)
- ✅ Document structure
- ✅ Establish scalar categories
- ✅ Define template pattern

### v2.1+
- 📋 Consider reducing scalar count if duplicates exist
- 📋 Evaluate performance of scalar resolution

---

## See Also

- **Main documentation**: `docs/ORGANIZATION.md`
- **Related tests**: `tests/unit/types/`
- **Decorators**: `src/fraiseql/decorators.py`
- **Core module**: `src/fraiseql/core/`

---

**Last Updated**: January 8, 2026
**Stability**: Stable
**Scalar Count**: 40+
