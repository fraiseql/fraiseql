# Phase 5: Documentation & Examples

## Objective

Provide comprehensive documentation, examples, and migration guides for the input normalization feature.

## Deliverables

### 1. Main Documentation Update
**File**: `docs/features/input-normalization.md` (NEW)

```markdown
# Input Normalization

FraiseQL provides built-in input normalization for mutations, allowing you to declaratively specify how input values should be transformed before reaching your database.

## Quick Start

### Field-Level Normalization

```python
from fraiseql import fraise_input, fraise_field

@fraise_input
class CreateUserInput:
    email: str = fraise_field(normalize=["trim", "lowercase"])
    name: str = fraise_field(normalize=["trim", "capitalize"])
```

### Type-Level Normalization

```python
@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateTagInput:
    tag: str  # Auto-normalized: trim + lowercase
    description: str  # Auto-normalized: trim + lowercase
```

### Global Normalization

```python
from fraiseql.config import set_global_config

set_global_config(
    default_string_normalization=["trim"],
    unicode_normalization="NFC"
)
```

## Available Normalizers

| Normalizer | Description | Example |
|------------|-------------|---------|
| `trim` | Remove leading/trailing whitespace | `"  hello  "` → `"hello"` |
| `lowercase` | Convert to lowercase | `"HELLO"` → `"hello"` |
| `uppercase` | Convert to uppercase | `"hello"` → `"HELLO"` |
| `capitalize` | Capitalize each word | `"hello world"` → `"Hello World"` |

## Unicode Normalization

```python
@fraise_input(unicode_form="NFC")
class UnicodeInput:
    name: str  # Unicode normalized to NFC form
```

Forms: `NFC`, `NFKC`, `NFD`, `NFKD`

## Normalization Priority

1. **Field-level** (highest): `fraise_field(normalize=[...])`
2. **Type-level**: `@fraise_input(normalize_strings=[...])`
3. **Global**: `SchemaConfig.default_string_normalization`
4. **Framework default** (lowest): `["trim"]`

## Validation (Optional)

```python
@fraise_input
class CreateUserInput:
    email: str = fraise_field(
        normalize=["trim", "lowercase"],
        validate={"email": True, "max_length": 255}
    )
    password: str = fraise_field(
        validate={"min_length": 8, "regex": r"^(?=.*[A-Z])(?=.*\d)"}
    )
```

## Migration Guide

### From Database-Level Normalization

**Before** (PostgreSQL helper):
```sql
CREATE FUNCTION trim_record_text_fields(rec jsonb) RETURNS jsonb AS $$
  -- Trim all text fields...
$$ LANGUAGE plpgsql;
```

**After** (FraiseQL normalization):
```python
@fraise_input(normalize_strings=["trim"])
class CreateUserInput:
    name: str
    email: str
```

### From `prepare_input()` Hook

**Before**:
```python
@mutation
class CreateUser:
    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        if "email" in input_data:
            input_data["email"] = input_data["email"].lower()
        return input_data
```

**After**:
```python
@fraise_input
class CreateUserInput:
    email: str = fraise_field(normalize=["trim", "lowercase"])
```

## Best Practices

1. **Use type-level normalization for consistency** across all fields
2. **Use field-level for exceptions** (e.g., display names need capitalize, not lowercase)
3. **Use global config for project-wide policies** (e.g., always trim, always NFC)
4. **Combine normalization with validation** for robust input handling
5. **Opt-out explicitly** with `normalize=False` when needed (e.g., raw JSON fields)
```

---

### 2. API Reference Update
**File**: `docs/api/fields.md`

Add documentation for:
- `fraise_field(normalize=...)` parameter
- `fraise_field(validate=...)` parameter

---

### 3. Examples
**Directory**: `examples/normalization/`

Create example files:

**`examples/normalization/basic_normalization.py`**:
```python
"""
Basic input normalization example.
"""

from fraiseql import fraise_input, fraise_field, mutation, success, failure


@fraise_input
class CreateUserInput:
    """User input with field-level normalization."""
    name: str = fraise_field(normalize=["trim", "capitalize"])
    email: str = fraise_field(normalize=["trim", "lowercase"])
    username: str = fraise_field(normalize=["trim", "lowercase"])


@success
class CreateUserSuccess:
    user_id: str
    message: str = "User created successfully"


@failure
class CreateUserError:
    pass


@mutation(function="create_user", schema="public")
class CreateUser:
    """Create a new user with normalized input."""
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError


# Usage:
# mutation {
#   createUser(input: {
#     name: "  john doe  "      # Will be normalized to "John Doe"
#     email: "  USER@EXAMPLE.COM  "  # Will be normalized to "user@example.com"
#     username: "  JohnDoe  "   # Will be normalized to "johndoe"
#   }) {
#     ... on CreateUserSuccess {
#       userId
#       message
#     }
#   }
# }
```

**`examples/normalization/type_level_normalization.py`**:
```python
"""
Type-level normalization example.
"""

from fraiseql import fraise_input, fraise_field, mutation, success, failure


@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateTagInput:
    """All string fields auto-normalized to lowercase."""
    tag: str  # Inherits: trim + lowercase
    category: str  # Inherits: trim + lowercase
    display_name: str = fraise_field(normalize=["capitalize"])  # Override


@success
class CreateTagSuccess:
    tag_id: str


@failure
class CreateTagError:
    pass


@mutation(function="create_tag")
class CreateTag:
    input: CreateTagInput
    success: CreateTagSuccess
    error: CreateTagError


# Usage:
# mutation {
#   createTag(input: {
#     tag: "  PYTHON  "          # → "python"
#     category: "  PROGRAMMING  " # → "programming"
#     displayName: "  python language  "  # → "Python Language"
#   }) {
#     ... on CreateTagSuccess {
#       tagId
#     }
#   }
# }
```

**`examples/normalization/validation_example.py`**:
```python
"""
Combined normalization and validation example.
"""

from fraiseql import fraise_input, fraise_field


@fraise_input
class CreateAccountInput:
    """Account input with normalization and validation."""

    email: str = fraise_field(
        normalize=["trim", "lowercase"],
        validate={"email": True, "max_length": 255}
    )

    password: str = fraise_field(
        validate={
            "min_length": 8,
            "max_length": 128,
            "regex": r"^(?=.*[A-Z])(?=.*[a-z])(?=.*\d)"
        }
    )

    age: int = fraise_field(
        validate={"min": 13, "max": 120}
    )


# Validation errors will be returned as GraphQL errors:
# {
#   "errors": [
#     {
#       "message": "email: Invalid email format",
#       "path": ["createAccount", "input", "email"]
#     }
#   ]
# }
```

---

### 4. Changelog Entry
**File**: `CHANGELOG.md`

```markdown
## [Unreleased]

### Added
- **Input Normalization Framework**:
  - Field-level normalization via `fraise_field(normalize=[...])`
  - Type-level normalization via `@fraise_input(normalize_strings=[...])`
  - Global normalization defaults via `SchemaConfig.default_string_normalization`
  - Built-in normalizers: trim, lowercase, uppercase, capitalize
  - Unicode normalization support (NFC, NFKC, NFD, NFKD)
  - Validation framework with `fraise_field(validate={...})`
  - String validators: min/max length, regex, email
  - Numeric validators: min/max value
  - Custom validator support

### Changed
- String trimming is now explicit via normalization framework (backward compatible)
```

---

### 5. Type Stubs Update
**File**: `src/fraiseql/fields.pyi`

```python
from typing import Any, Callable, Literal, Sequence

def fraise_field(
    *,
    default: Any = ...,
    default_factory: Callable[[], Any] | None = None,
    description: str | None = None,
    purpose: Literal["input", "output", "both"] | None = None,
    deprecation_reason: str | None = None,
    directives: Sequence[object] = (),
    metadata: dict[str, Any] | None = None,
    normalize: list[str] | Literal[False] | None = None,
    validate: dict[str, Any] | None = None,
) -> Any: ...
```

---

### 6. README Update
**File**: `README.md`

Add to features section:

```markdown
## Features

- **Database-first GraphQL**: Generate GraphQL schemas from PostgreSQL functions
- **Type-safe mutations**: Automatic input/output type generation
- **Input normalization**: Built-in string normalization (trim, lowercase, etc.)
- **Validation framework**: Declarative input validation
- **Error handling**: Structured error responses
- **Batch operations**: Efficient bulk mutations
```

---

## Acceptance Criteria

- [ ] Main documentation complete with examples
- [ ] API reference updated with new parameters
- [ ] Example files created and tested
- [ ] Migration guide provided
- [ ] Changelog updated
- [ ] Type stubs updated for IDE autocomplete
- [ ] README mentions new feature

## Verification Commands

```bash
# Verify examples run
python examples/normalization/basic_normalization.py

# Verify type stubs
mypy examples/normalization/ --strict

# Build documentation
cd docs && make html

# Spell check documentation
aspell check docs/features/input-normalization.md
```

## Estimated Time

**2-3 hours**

## Notes

1. Documentation should include real-world use cases
2. Examples should be runnable and well-commented
3. Migration guide should cover common patterns
4. API reference should include all parameters and default values
5. Type stubs ensure IDE autocomplete works correctly
