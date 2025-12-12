# Phase 4: Validation Framework (Optional)

## Objective

Add optional validation support to `fraise_field()` for **domain-specific constraints** (min/max length, regex patterns), with integration into GraphQL error handling.

**Note**: FraiseQL already has rich type validation (custom scalars for email, phone, IP addresses, etc.). This phase focuses on **length constraints** and **pattern matching** that complement type validation.

## Context

Normalization ensures data is consistently formatted, but **validation ensures data is correct**. This phase adds declarative validation:

```python
@fraise_input
class CreateUserInput:
    # Use FraiseQL's Email scalar for type validation
    email: Email = fraise_field(
        validate={"max_length": 255}  # Additional domain constraint
    )

    # Password: length + pattern validation
    password: str = fraise_field(
        validate={
            "min_length": 8,
            "max_length": 128,
            "regex": r"^(?=.*[A-Z])(?=.*[a-z])(?=.*\d)"  # Require uppercase, lowercase, digit
        }
    )

    # Username: length constraint
    username: str = fraise_field(
        normalize=["trim", "lowercase"],
        validate={"min_length": 3, "max_length": 30}
    )
```

## Architecture

### Validation Flow

```
GraphQL Input
    ↓
Coercion (coercion.py)
    ↓
prepare_input() hook
    ↓
Serialization (_serialize_value)
    ↓
  [NEW] Apply normalization
    ↓
  [NEW] Apply validation
    ↓
  (If validation fails → raise GraphQLError)
    ↓
JSONB serialization
```

### Validation Timing

**After normalization, before database call**:
1. Normalize: `"  USER@EXAMPLE.COM  "` → `"user@example.com"`
2. Validate: Check email format on normalized value
3. Serialize: Convert to JSONB

## Scope

**In Scope** (Domain Constraints):
- ✅ String length validation (min_length, max_length)
- ✅ Regex pattern matching
- ✅ Custom validators (callable functions)

**Out of Scope** (Already Handled by FraiseQL):
- ❌ Email validation (use `Email` scalar)
- ❌ Phone validation (use `PhoneNumber` scalar)
- ❌ IP address validation (use `IPv4Address`/`IPv6Address` scalars)
- ❌ Date/time validation (use `DateTime`/`Date` scalars)
- ❌ Numeric range validation (use custom scalars if needed)

## Files to Modify

1. **`src/fraiseql/validation.py`** - NEW: Validation functions (simplified)
2. **`src/fraiseql/fields.py`** - Add `validate` parameter (already stubbed in Phase 1)
3. **`src/fraiseql/mutations/sql_generator.py`** - Apply validation in `_serialize_value()`
4. **`tests/test_validation.py`** - Unit tests
5. **`tests/integration/test_mutation_validation.py`** - Integration tests

## Implementation Steps

### Step 1: Create Validation Module
**File**: `src/fraiseql/validation.py`

```python
"""
Input validation functions for FraiseQL mutations.

This module provides domain-specific validation (length, patterns) that complements
FraiseQL's type system (Email, PhoneNumber, IPv4Address, etc. scalars).
"""

import re
from typing import Any, Callable


class ValidationError(Exception):
    """Raised when input validation fails."""

    def __init__(self, field_name: str, message: str):
        self.field_name = field_name
        self.message = message
        super().__init__(f"{field_name}: {message}")


# String validators

def validate_min_length(value: str, min_length: int, field_name: str) -> None:
    """Validate minimum string length."""
    if len(value) < min_length:
        raise ValidationError(
            field_name,
            f"Must be at least {min_length} characters (got {len(value)})"
        )


def validate_max_length(value: str, max_length: int, field_name: str) -> None:
    """Validate maximum string length."""
    if len(value) > max_length:
        raise ValidationError(
            field_name,
            f"Must be at most {max_length} characters (got {len(value)})"
        )


def validate_regex(value: str, pattern: str, field_name: str) -> None:
    """
    Validate string matches regex pattern.

    Args:
        value: String to validate
        pattern: Regex pattern
        field_name: Field name for error messages

    Raises:
        ValidationError: If value doesn't match pattern

    Examples:
        >>> validate_regex("Password123", r"^(?=.*[A-Z])(?=.*\d)", "password")  # OK
        >>> validate_regex("password", r"^(?=.*[A-Z])(?=.*\d)", "password")  # Error
    """
    try:
        compiled_pattern = re.compile(pattern)
    except re.error as e:
        raise ValueError(f"Invalid regex pattern: {pattern} ({e})")

    if not compiled_pattern.match(value):
        raise ValidationError(
            field_name,
            f"Must match pattern: {pattern}"
        )


def apply_validation(
    value: Any,
    rules: dict[str, Any],
    field_name: str,
) -> None:
    """
    Apply validation rules to a value.

    Args:
        value: Value to validate
        rules: Dict of validation rules
        field_name: Field name for error messages

    Raises:
        ValidationError: If validation fails

    Supported rules for strings:
        - min_length: Minimum string length
        - max_length: Maximum string length
        - regex: Regex pattern to match
        - custom: Callable validator function

    Examples:
        >>> apply_validation("password123", {"min_length": 8, "max_length": 128}, "password")
        >>> apply_validation("short", {"min_length": 10}, "password")
        ValidationError: password: Must be at least 10 characters

        >>> apply_validation("Password123", {"regex": r"^(?=.*[A-Z])(?=.*\d)"}, "password")
    """
    # String validation
    if isinstance(value, str):
        if "min_length" in rules:
            validate_min_length(value, rules["min_length"], field_name)
        if "max_length" in rules:
            validate_max_length(value, rules["max_length"], field_name)
        if "regex" in rules:
            validate_regex(value, rules["regex"], field_name)

    # Custom validator (callable)
    if "custom" in rules:
        validator = rules["custom"]
        if callable(validator):
            validator(value, field_name)
        else:
            raise TypeError(f"Custom validator must be callable, got {type(validator)}")
```

### Step 2: Apply Validation in Serialization
**File**: `src/fraiseql/mutations/sql_generator.py`

```python
def _serialize_value(
    value: Any,
    field_type: Any = None,
    field_name: str | None = None,
    input_class: type | None = None,
) -> Any:
    """Serialize and validate a value for JSONB storage."""
    from fraiseql.validation import apply_validation, ValidationError

    if value is UNSET:
        return None

    # Get field metadata
    field_metadata = _get_field_metadata(field_type, field_name, input_class)

    if isinstance(value, str):
        # Step 1: Apply normalization
        result = _apply_normalization(value, field_metadata)

        # Step 2: Apply validation on normalized value
        if "validate" in field_metadata:
            try:
                apply_validation(result, field_metadata["validate"], field_name or "field")
            except ValidationError as e:
                # Convert to GraphQL error
                from strawberry.exceptions import StrawberryGraphQLError
                raise StrawberryGraphQLError(e.message, path=[e.field_name])

        # Step 3: Convert empty string to None
        return None if result == "" else result

    # [Rest of serialization unchanged - validation only for strings in this phase]
```

### Step 3: Write Tests
**File**: `tests/test_validation.py`

```python
import pytest
from fraiseql.validation import (
    apply_validation,
    validate_min_length,
    validate_max_length,
    validate_regex,
    ValidationError,
)


class TestValidationFunctions:
    """Test individual validation functions."""

    def test_min_length(self):
        validate_min_length("hello", 5, "field")  # OK
        with pytest.raises(ValidationError):
            validate_min_length("hi", 5, "field")

    def test_max_length(self):
        validate_max_length("hello", 10, "field")  # OK
        with pytest.raises(ValidationError):
            validate_max_length("hello world", 5, "field")

    def test_regex_valid(self):
        # Valid password: uppercase, lowercase, digit
        validate_regex("Password123", r"^(?=.*[A-Z])(?=.*[a-z])(?=.*\d)", "password")  # OK

    def test_regex_invalid(self):
        # Invalid: missing uppercase
        with pytest.raises(ValidationError):
            validate_regex("password123", r"^(?=.*[A-Z])(?=.*[a-z])(?=.*\d)", "password")

    def test_regex_invalid_pattern(self):
        # Invalid regex pattern
        with pytest.raises(ValueError, match="Invalid regex pattern"):
            validate_regex("test", r"(?P<invalid", "field")


class TestApplyValidation:
    """Test composite validation."""

    def test_length_validation(self):
        # Valid: within length bounds
        apply_validation("password123", {"min_length": 8, "max_length": 128}, "password")  # OK

        # Invalid: too short
        with pytest.raises(ValidationError, match="at least 8"):
            apply_validation("short", {"min_length": 8}, "password")

        # Invalid: too long
        with pytest.raises(ValidationError, match="at most 20"):
            apply_validation("a" * 21, {"max_length": 20}, "field")

    def test_regex_validation(self):
        # Valid password
        apply_validation(
            "Password123",
            {"regex": r"^(?=.*[A-Z])(?=.*[a-z])(?=.*\d)"},
            "password"
        )  # OK

        # Invalid password (missing uppercase)
        with pytest.raises(ValidationError, match="Must match pattern"):
            apply_validation("password123", {"regex": r"^(?=.*[A-Z])"}, "password")

    def test_combined_validation(self):
        # Valid: meets all criteria
        apply_validation(
            "Password123",
            {"min_length": 8, "max_length": 128, "regex": r"^(?=.*[A-Z])(?=.*\d)"},
            "password"
        )  # OK

        # Invalid: too short
        with pytest.raises(ValidationError, match="at least 8"):
            apply_validation(
                "Pass1",
                {"min_length": 8, "regex": r"^(?=.*[A-Z])(?=.*\d)"},
                "password"
            )

    def test_custom_validator(self):
        # Custom validator: must not contain "admin"
        def no_admin(value: str, field_name: str):
            if "admin" in value.lower():
                raise ValidationError(field_name, "Username cannot contain 'admin'")

        apply_validation("john_doe", {"custom": no_admin}, "username")  # OK

        with pytest.raises(ValidationError, match="cannot contain 'admin'"):
            apply_validation("admin_user", {"custom": no_admin}, "username")
```

## Acceptance Criteria

- [ ] `fraise_field(validate={...})` parameter works
- [ ] String validators work (min/max length, regex)
- [ ] Custom validators (callable) work
- [ ] Validation errors are converted to GraphQL errors
- [ ] Validation happens after normalization (normalized value is validated)
- [ ] All tests pass
- [ ] Clear error messages for validation failures
- [ ] Invalid regex patterns raise helpful errors

## Verification Commands

```bash
# Test validation
python -c "
from fraiseql.validation import apply_validation, ValidationError

try:
    apply_validation('short', {'min_length': 10}, 'password')
    print('FAIL')
except ValidationError as e:
    print('OK: Validation works')
"

# Run tests
uv run pytest tests/test_validation.py -v
```

## Estimated Time

**2-3 hours** (reduced from 4-5 due to narrower scope)

## Notes

1. **Scope**: Only string length and regex validation (numeric validation removed since FraiseQL has rich scalar types)
2. **Type validation vs domain validation**:
   - **Type validation** (FraiseQL scalars): Email, PhoneNumber, IPv4Address, DateTime, etc.
   - **Domain validation** (this phase): Length constraints, pattern matching, business rules
3. **Error handling**: Integrates with GraphQL error format (path, message)
4. **Custom validators**: Allow callable validators for complex business logic
5. **Performance**: Zero overhead when `validate` parameter not provided

## Next Phase

**Phase 5**: Documentation & Examples
