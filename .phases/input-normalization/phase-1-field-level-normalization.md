# Phase 1: Field-Level Normalization (Core)

## Objective

Implement field-level input normalization API with support for common string transformations (trim, lowercase, uppercase, capitalize) and unicode normalization (NFC, NFKC, NFD, NFKD).

## Context

FraiseQL currently performs automatic string trimming in `_serialize_value()` (sql_generator.py:46), but this is:
1. **Implicit** - no way to disable or customize
2. **Limited** - only whitespace trimming, no other transformations
3. **Not declarative** - developers must use `prepare_input()` hook for custom normalization

This phase adds **declarative field-level normalization** to make normalization explicit, configurable, and composable.

## Architecture

### Normalization Flow

```
GraphQL Input
    ↓
Coercion (coercion.py)
    ↓
prepare_input() hook (optional)
    ↓
Serialization (_serialize_value)
    ↓
  [NEW] Apply field-level normalization
    ↓
JSONB serialization
    ↓
PostgreSQL function call
```

### Key Design Decisions

1. **Normalization happens in serialization layer** (`_serialize_value()`)
   - Central location for all input transformations
   - Consistent application across all mutation types
   - Type-aware normalization (only applies to strings)

2. **Field metadata stores normalization rules** (`fraise_field()`)
   - Declarative API: `fraise_field(normalize=["trim", "lowercase"])`
   - Composable: multiple rules applied in order
   - Explicit: clear error messages for invalid rules

3. **Normalization is opt-in** (except trim, which is current behavior)
   - Backward compatible with existing mutations
   - Zero performance overhead when not used
   - Can opt-out of trim with `normalize=False`

## Files to Modify

### 1. `src/fraiseql/fields.py` (Extend `fraise_field()`)
**Lines**: 13-180

**Changes**:
- Add `normalize` parameter to `fraise_field()` function
- Add `validate` parameter (for Phase 4)
- Store normalization rules in field metadata
- Validate normalization rules (ensure valid rule names)

### 2. `src/fraiseql/mutations/sql_generator.py` (Normalization logic)
**Lines**: 29-74 (`_serialize_value` function)

**Changes**:
- Create `_apply_normalization()` helper function
- Call `_apply_normalization()` in `_serialize_value()` for string values
- Retrieve normalization rules from field metadata
- Apply normalization rules in order

### 3. `src/fraiseql/normalization.py` (NEW FILE)
Create new module for normalization functions.

**Contents**:
- Normalization function registry
- String normalization functions (trim, lowercase, uppercase, capitalize)
- Unicode normalization functions (NFC, NFKC, NFD, NFKD)
- Normalization rule validation

### 4. `tests/test_normalization.py` (NEW FILE)
Unit tests for normalization functions.

### 5. `tests/integration/test_mutation_normalization.py` (NEW FILE)
Integration tests with real mutations.

## Implementation Steps

### Step 1: Create Normalization Module
**File**: `src/fraiseql/normalization.py`

```python
"""
Input normalization functions for FraiseQL mutations.

This module provides declarative input normalization for mutation input fields.
Normalization is applied during serialization, before values are sent to PostgreSQL.
"""

import unicodedata
from typing import Callable, Literal

# Type alias for normalization rule names
NormalizationRule = Literal["trim", "lowercase", "uppercase", "capitalize", "strip"]

# Type alias for unicode normalization forms
UnicodeNormalization = Literal["NFC", "NFKC", "NFD", "NFKD"]


def normalize_trim(value: str) -> str:
    """Remove leading and trailing whitespace."""
    return value.strip()


def normalize_lowercase(value: str) -> str:
    """Convert string to lowercase."""
    return value.lower()


def normalize_uppercase(value: str) -> str:
    """Convert string to uppercase."""
    return value.upper()


def normalize_capitalize(value: str) -> str:
    """Capitalize first letter of each word."""
    return value.title()


def normalize_strip(value: str) -> str:
    """Alias for trim (strip whitespace)."""
    return value.strip()


def normalize_unicode(value: str, form: UnicodeNormalization = "NFC") -> str:
    """
    Normalize unicode string to specified form.

    Args:
        value: String to normalize
        form: Unicode normalization form (NFC, NFKC, NFD, NFKD)
            - NFC: Canonical Decomposition, followed by Canonical Composition
            - NFD: Canonical Decomposition
            - NFKC: Compatibility Decomposition, followed by Canonical Composition
            - NFKD: Compatibility Decomposition

    Returns:
        Normalized string

    Examples:
        >>> normalize_unicode("café", "NFC")  # é as single character
        'café'
        >>> normalize_unicode("café", "NFD")  # é as e + combining accent
        'café'
    """
    return unicodedata.normalize(form, value)


# Registry of normalization functions
NORMALIZATION_FUNCTIONS: dict[str, Callable[[str], str]] = {
    "trim": normalize_trim,
    "lowercase": normalize_lowercase,
    "uppercase": normalize_uppercase,
    "capitalize": normalize_capitalize,
    "strip": normalize_strip,  # Alias for trim
}


def apply_normalization(
    value: str,
    rules: list[str] | Literal[False] | None,
    unicode_form: UnicodeNormalization | None = None,
) -> str:
    """
    Apply normalization rules to a string value.

    Args:
        value: String to normalize
        rules: List of normalization rule names, or False to skip normalization
        unicode_form: Optional unicode normalization form

    Returns:
        Normalized string

    Raises:
        ValueError: If an invalid normalization rule is provided

    Examples:
        >>> apply_normalization("  HELLO  ", ["trim", "lowercase"])
        'hello'
        >>> apply_normalization("café", None, unicode_form="NFC")
        'café'
        >>> apply_normalization("  test  ", False)
        '  test  '
    """
    # If normalization explicitly disabled, return value unchanged
    if rules is False:
        return value

    # Apply unicode normalization first (if specified)
    if unicode_form:
        value = normalize_unicode(value, unicode_form)

    # If no rules specified, return value (possibly unicode-normalized)
    if not rules:
        return value

    # Apply normalization rules in order
    for rule in rules:
        if rule not in NORMALIZATION_FUNCTIONS:
            valid_rules = ", ".join(NORMALIZATION_FUNCTIONS.keys())
            raise ValueError(
                f"Invalid normalization rule: {rule!r}. "
                f"Valid rules: {valid_rules}"
            )

        normalizer = NORMALIZATION_FUNCTIONS[rule]
        value = normalizer(value)

    return value


def validate_normalization_rules(rules: list[str] | Literal[False] | None) -> None:
    """
    Validate normalization rules.

    Args:
        rules: List of normalization rule names, or False to skip normalization

    Raises:
        ValueError: If an invalid normalization rule is provided
        TypeError: If rules is not a list, False, or None
    """
    if rules is False or rules is None:
        return

    if not isinstance(rules, list):
        raise TypeError(
            f"normalize parameter must be a list of rule names, False, or None. "
            f"Got: {type(rules).__name__}"
        )

    for rule in rules:
        if rule not in NORMALIZATION_FUNCTIONS:
            valid_rules = ", ".join(NORMALIZATION_FUNCTIONS.keys())
            raise ValueError(
                f"Invalid normalization rule: {rule!r}. "
                f"Valid rules: {valid_rules}"
            )
```

**Verification**:
```bash
# Test imports
python -c "from fraiseql.normalization import apply_normalization, NORMALIZATION_FUNCTIONS; print('OK')"

# Test basic normalization
python -c "from fraiseql.normalization import apply_normalization; print(apply_normalization('  HELLO  ', ['trim', 'lowercase']))"
# Expected: hello

# Test unicode normalization
python -c "from fraiseql.normalization import normalize_unicode; print(repr(normalize_unicode('café', 'NFC')))"
# Expected: 'café'
```

---

### Step 2: Extend `fraise_field()` with Normalization Parameters
**File**: `src/fraiseql/fields.py`

**Current signature** (line 13):
```python
def fraise_field(
    *,
    default: Any = UNSET,
    default_factory: Callable[[], Any] | None = None,
    description: str | None = None,
    purpose: Literal["input", "output", "both"] | None = None,
    deprecation_reason: str | None = None,
    directives: Sequence[object] = (),
    metadata: dict[str, Any] | None = None,
) -> Any:
```

**New signature**:
```python
def fraise_field(
    *,
    default: Any = UNSET,
    default_factory: Callable[[], Any] | None = None,
    description: str | None = None,
    purpose: Literal["input", "output", "both"] | None = None,
    deprecation_reason: str | None = None,
    directives: Sequence[object] = (),
    metadata: dict[str, Any] | None = None,
    # NEW PARAMETERS
    normalize: list[str] | Literal[False] | None = None,
    validate: dict[str, Any] | None = None,  # For Phase 4
) -> Any:
```

**Changes**:

1. **Add imports** (top of file):
```python
from fraiseql.normalization import validate_normalization_rules
```

2. **Update function body** (after line 13):
```python
def fraise_field(
    *,
    default: Any = UNSET,
    default_factory: Callable[[], Any] | None = None,
    description: str | None = None,
    purpose: Literal["input", "output", "both"] | None = None,
    deprecation_reason: str | None = None,
    directives: Sequence[object] = (),
    metadata: dict[str, Any] | None = None,
    normalize: list[str] | Literal[False] | None = None,
    validate: dict[str, Any] | None = None,
) -> Any:
    """
    Create a field with FraiseQL-specific metadata.

    Args:
        default: Default value for the field
        default_factory: Factory function to generate default values
        description: Field description for GraphQL schema
        purpose: Whether field is used for input, output, or both
        deprecation_reason: Deprecation message for GraphQL schema
        directives: GraphQL directives to apply
        metadata: Additional metadata
        normalize: Normalization rules to apply (e.g., ["trim", "lowercase"])
                   Set to False to disable all normalization (including default trim)
                   Valid rules: trim, lowercase, uppercase, capitalize, strip
        validate: Validation rules (for Phase 4)

    Returns:
        Field with metadata attached

    Examples:
        >>> fraise_field(normalize=["trim", "lowercase"])
        >>> fraise_field(normalize=False)  # Disable normalization
        >>> fraise_field(default="", normalize=["trim"])
    """
    # Validate normalization rules
    if normalize is not None:
        validate_normalization_rules(normalize)

    # Create metadata dict
    if metadata is None:
        metadata = {}

    # Store normalization rules in metadata
    if normalize is not None:
        metadata["normalize"] = normalize

    # Store validation rules in metadata (Phase 4)
    if validate is not None:
        metadata["validate"] = validate

    # [Rest of existing function logic remains unchanged]
    ...
```

**Verification**:
```bash
# Test field creation with normalization
python -c "
from fraiseql import fraise_field
field = fraise_field(normalize=['trim', 'lowercase'])
print('OK')
"

# Test invalid normalization rule
python -c "
from fraiseql import fraise_field
try:
    field = fraise_field(normalize=['invalid_rule'])
    print('FAIL: Should raise ValueError')
except ValueError as e:
    print('OK: Validation works')
"
```

---

### Step 3: Integrate Normalization into `_serialize_value()`
**File**: `src/fraiseql/mutations/sql_generator.py`

**Current code** (lines 39-74):
```python
def _serialize_value(value: Any, field_type: Any = None) -> Any:
    """Serialize a value for JSONB storage."""
    if value is UNSET:
        return None

    if isinstance(value, str):
        result = value.strip()  # Current trimming behavior
        return None if result == "" else result

    # [Rest of function...]
```

**New code** (with normalization):

1. **Add imports** (top of file):
```python
from fraiseql.normalization import apply_normalization
from fraiseql.fields import UNSET
import inspect
```

2. **Add helper function** (after imports, before `_serialize_value`):
```python
def _get_field_metadata(field_type: Any, field_name: str | None = None) -> dict[str, Any]:
    """
    Extract metadata from a field type.

    Args:
        field_type: The field type (from type annotations)
        field_name: Optional field name for debugging

    Returns:
        Field metadata dict (empty if no metadata found)
    """
    # Try to get metadata from field_type
    if hasattr(field_type, "metadata"):
        return field_type.metadata or {}

    # Try to get from __metadata__ (for Annotated types)
    if hasattr(field_type, "__metadata__"):
        for item in field_type.__metadata__:
            if isinstance(item, dict):
                return item

    return {}


def _apply_normalization(
    value: str,
    field_metadata: dict[str, Any],
) -> str:
    """
    Apply normalization rules from field metadata.

    Args:
        value: String value to normalize
        field_metadata: Field metadata containing normalization rules

    Returns:
        Normalized string
    """
    # Get normalization rules from metadata
    normalize_rules = field_metadata.get("normalize")

    # If normalize explicitly set to False, skip normalization
    if normalize_rules is False:
        return value

    # If no rules specified, apply default trim (current behavior)
    if normalize_rules is None:
        normalize_rules = ["trim"]

    # Apply normalization
    return apply_normalization(value, normalize_rules)
```

3. **Update `_serialize_value()`** (replace lines 39-74):
```python
def _serialize_value(value: Any, field_type: Any = None, field_name: str | None = None) -> Any:
    """
    Serialize a value for JSONB storage.

    Args:
        value: Value to serialize
        field_type: Type annotation for the field (used to extract metadata)
        field_name: Optional field name for debugging

    Returns:
        Serialized value suitable for JSONB
    """
    if value is UNSET:
        return None

    if isinstance(value, str):
        # Get field metadata (if available)
        field_metadata = _get_field_metadata(field_type, field_name) if field_type else {}

        # Apply normalization (includes default trim behavior)
        result = _apply_normalization(value, field_metadata)

        # Convert empty string to None (existing behavior)
        return None if result == "" else result

    if isinstance(value, UUID):
        return str(value)

    if isinstance(value, (datetime, date)):
        return value.isoformat()

    if isinstance(value, Enum):
        return value.value

    if isinstance(value, (IPv4Address, IPv6Address)):
        return str(value)

    if isinstance(value, dict):
        # Recursively serialize dict values
        return {
            dict_key_to_snake_case(k): _serialize_value(v, field_name=k)
            for k, v in value.items()
        }

    if isinstance(value, (list, tuple)):
        # Filter out UNSET items and recursively serialize
        return [
            _serialize_value(item)
            for item in value
            if item is not UNSET
        ]

    # Handle dataclasses and FraiseQL input objects
    if hasattr(value, "__dataclass_fields__") or hasattr(value, "__annotations__"):
        result = {}

        # Get field annotations
        annotations = getattr(value, "__annotations__", {})

        for key, field_type in annotations.items():
            if hasattr(value, key):
                field_value = getattr(value, key)
                if field_value is not UNSET:
                    result[dict_key_to_snake_case(key)] = _serialize_value(
                        field_value,
                        field_type=field_type,
                        field_name=key
                    )

        return result

    # Return value unchanged for other types
    return value
```

**Key Changes**:
- Added `field_type` and `field_name` parameters to `_serialize_value()`
- Extract field metadata using `_get_field_metadata()`
- Apply normalization using `_apply_normalization()`
- Default behavior: trim (backward compatible with current behavior)
- Can opt-out with `normalize=False`

**Verification**:
```bash
# Test serialization with normalization
python -c "
from fraiseql.mutations.sql_generator import _serialize_value
from fraiseql import fraise_field

# Test default trim behavior
result = _serialize_value('  hello  ')
assert result == 'hello', f'Expected hello, got {result}'
print('OK: Default trim works')

# Test with field metadata
# [More complex test with actual field types]
"
```

---

### Step 4: Update Input Serialization to Pass Field Types
**File**: `src/fraiseql/mutations/sql_generator.py`

The `_serialize_value()` function is called from multiple places. We need to ensure field type information is passed through:

**Locations to update**:

1. **Line ~65**: Dict serialization
```python
# BEFORE
return {
    dict_key_to_snake_case(k): _serialize_value(v)
    for k, v in value.items()
}

# AFTER
return {
    dict_key_to_snake_case(k): _serialize_value(v, field_name=k)
    for k, v in value.items()
}
```

2. **Line ~70**: List serialization
```python
# Already correct (no field type needed for list items)
return [
    _serialize_value(item)
    for item in value
    if item is not UNSET
]
```

3. **Line ~80**: Dataclass/input object serialization
```python
# BEFORE
for key in annotations:
    if hasattr(value, key):
        field_value = getattr(value, key)
        if field_value is not UNSET:
            result[dict_key_to_snake_case(key)] = _serialize_value(field_value)

# AFTER
for key, field_type in annotations.items():
    if hasattr(value, key):
        field_value = getattr(value, key)
        if field_value is not UNSET:
            result[dict_key_to_snake_case(key)] = _serialize_value(
                field_value,
                field_type=field_type,
                field_name=key
            )
```

---

### Step 5: Write Unit Tests
**File**: `tests/test_normalization.py`

```python
"""
Unit tests for input normalization functions.
"""

import pytest
from fraiseql.normalization import (
    apply_normalization,
    normalize_trim,
    normalize_lowercase,
    normalize_uppercase,
    normalize_capitalize,
    normalize_unicode,
    validate_normalization_rules,
)


class TestNormalizationFunctions:
    """Test individual normalization functions."""

    def test_normalize_trim(self):
        assert normalize_trim("  hello  ") == "hello"
        assert normalize_trim("hello") == "hello"
        assert normalize_trim("  ") == ""
        assert normalize_trim("") == ""

    def test_normalize_lowercase(self):
        assert normalize_lowercase("HELLO") == "hello"
        assert normalize_lowercase("Hello") == "hello"
        assert normalize_lowercase("hello") == "hello"
        assert normalize_lowercase("HeLLo WoRLd") == "hello world"

    def test_normalize_uppercase(self):
        assert normalize_uppercase("hello") == "HELLO"
        assert normalize_uppercase("Hello") == "HELLO"
        assert normalize_uppercase("HELLO") == "HELLO"
        assert normalize_uppercase("hello world") == "HELLO WORLD"

    def test_normalize_capitalize(self):
        assert normalize_capitalize("hello world") == "Hello World"
        assert normalize_capitalize("HELLO WORLD") == "Hello World"
        assert normalize_capitalize("hello") == "Hello"

    def test_normalize_unicode_nfc(self):
        # Composed form (single character)
        assert normalize_unicode("café", "NFC") == "café"
        # Should normalize decomposed to composed
        decomposed = "cafe\u0301"  # e + combining acute accent
        assert normalize_unicode(decomposed, "NFC") == "café"

    def test_normalize_unicode_nfd(self):
        # Should decompose to base + combining characters
        result = normalize_unicode("café", "NFD")
        assert len(result) == 5  # c, a, f, e, combining-acute
        assert result[3] == "e"
        assert result[4] == "\u0301"  # combining acute accent


class TestApplyNormalization:
    """Test composite normalization application."""

    def test_single_rule(self):
        assert apply_normalization("  hello  ", ["trim"]) == "hello"
        assert apply_normalization("HELLO", ["lowercase"]) == "hello"

    def test_multiple_rules(self):
        # Order matters: trim first, then lowercase
        assert apply_normalization("  HELLO  ", ["trim", "lowercase"]) == "hello"
        assert apply_normalization("  hello world  ", ["trim", "capitalize"]) == "Hello World"

    def test_no_rules(self):
        # None or empty list = no normalization (return unchanged)
        assert apply_normalization("  HELLO  ", None) == "  HELLO  "
        assert apply_normalization("  HELLO  ", []) == "  HELLO  "

    def test_disabled_normalization(self):
        # False = explicitly disabled
        assert apply_normalization("  HELLO  ", False) == "  HELLO  "

    def test_unicode_normalization(self):
        # Unicode normalization applied first
        result = apply_normalization("  café  ", ["trim"], unicode_form="NFC")
        assert result == "café"

    def test_invalid_rule(self):
        with pytest.raises(ValueError, match="Invalid normalization rule"):
            apply_normalization("hello", ["invalid_rule"])


class TestValidateNormalizationRules:
    """Test normalization rule validation."""

    def test_valid_rules(self):
        # Should not raise
        validate_normalization_rules(["trim", "lowercase"])
        validate_normalization_rules(["uppercase"])
        validate_normalization_rules([])
        validate_normalization_rules(None)
        validate_normalization_rules(False)

    def test_invalid_rule(self):
        with pytest.raises(ValueError, match="Invalid normalization rule"):
            validate_normalization_rules(["invalid_rule"])

    def test_invalid_type(self):
        with pytest.raises(TypeError, match="must be a list"):
            validate_normalization_rules("trim")  # Should be ["trim"]


class TestEdgeCases:
    """Test edge cases and special characters."""

    def test_empty_string(self):
        assert apply_normalization("", ["trim", "lowercase"]) == ""

    def test_whitespace_only(self):
        assert apply_normalization("   ", ["trim"]) == ""

    def test_unicode_characters(self):
        # Emojis
        assert apply_normalization("  👋 Hello  ", ["trim"]) == "👋 Hello"

        # Japanese characters
        assert apply_normalization("  こんにちは  ", ["trim"]) == "こんにちは"

        # Arabic
        assert apply_normalization("  مرحبا  ", ["trim"]) == "مرحبا"

    def test_newlines_and_tabs(self):
        assert apply_normalization("  hello\n\tworld  ", ["trim"]) == "hello\n\tworld"

    def test_multiple_spaces(self):
        # Trim only removes leading/trailing, not internal spaces
        assert apply_normalization("  hello   world  ", ["trim"]) == "hello   world"
```

**Verification**:
```bash
# Run unit tests
uv run pytest tests/test_normalization.py -v

# Expected output:
# tests/test_normalization.py::TestNormalizationFunctions::test_normalize_trim PASSED
# tests/test_normalization.py::TestNormalizationFunctions::test_normalize_lowercase PASSED
# [... all tests pass ...]
```

---

### Step 6: Write Integration Tests
**File**: `tests/integration/test_mutation_normalization.py`

```python
"""
Integration tests for mutation input normalization.
"""

import pytest
from fraiseql import fraise_input, fraise_field, mutation, success, failure


@fraise_input
class CreateUserInput:
    """Input with field-level normalization."""

    name: str = fraise_field(normalize=["trim", "capitalize"])
    email: str = fraise_field(normalize=["trim", "lowercase"])
    username: str = fraise_field(normalize=["trim"])
    notes: str = fraise_field(normalize=False)  # Opt-out of normalization
    code: str = fraise_field(normalize=["trim", "uppercase"])


@success
class CreateUserSuccess:
    user_id: str


@failure
class CreateUserError:
    pass


@pytest.fixture
def mock_db_function(monkeypatch):
    """Mock PostgreSQL function call to inspect serialized input."""
    captured_input = {}

    async def mock_execute(query: str, params: tuple):
        """Capture the JSONB input sent to database."""
        import json

        # Extract JSONB parameter (last param)
        jsonb_param = params[-1]
        captured_input["data"] = json.loads(jsonb_param)

        # Return mock success response
        return [
            {
                "row_to_json": {
                    "status": "created",
                    "message": "User created",
                    "entity": {"id": "123"},
                    "entity_type": "User",
                }
            }
        ]

    # Monkeypatch the database executor
    # [Implementation depends on FraiseQL's internal structure]

    return captured_input


class TestFieldLevelNormalization:
    """Test field-level normalization in real mutations."""

    @pytest.mark.asyncio
    async def test_trim_and_capitalize(self, mock_db_function):
        """Test trim + capitalize normalization."""
        # Create mutation (using mutation decorator)
        @mutation(function="create_user")
        class CreateUser:
            input: CreateUserInput
            success: CreateUserSuccess
            error: CreateUserError

        # Execute mutation with raw input
        # [Actual execution depends on FraiseQL test setup]

        # Assert: Input should be normalized
        assert mock_db_function["data"]["name"] == "John Doe"  # Capitalized

    @pytest.mark.asyncio
    async def test_trim_and_lowercase(self, mock_db_function):
        """Test trim + lowercase normalization."""
        # Input: "  USER@EXAMPLE.COM  "
        # Expected: "user@example.com"

        # [Test execution]

        assert mock_db_function["data"]["email"] == "user@example.com"

    @pytest.mark.asyncio
    async def test_opt_out_normalization(self, mock_db_function):
        """Test normalize=False (opt-out)."""
        # Input: "  Important Notes  " (should NOT be trimmed)
        # Expected: "  Important Notes  " (unchanged)

        # [Test execution]

        assert mock_db_function["data"]["notes"] == "  Important Notes  "

    @pytest.mark.asyncio
    async def test_uppercase_normalization(self, mock_db_function):
        """Test uppercase normalization."""
        # Input: "  abc123  "
        # Expected: "ABC123"

        # [Test execution]

        assert mock_db_function["data"]["code"] == "ABC123"


class TestBackwardCompatibility:
    """Test backward compatibility with existing behavior."""

    @pytest.mark.asyncio
    async def test_default_trim_behavior(self, mock_db_function):
        """Test that default behavior is still trim (backward compatible)."""

        @fraise_input
        class LegacyInput:
            name: str  # No normalization specified (should default to trim)

        # Input: "  hello  "
        # Expected: "hello" (trimmed by default)

        # [Test execution]

        assert mock_db_function["data"]["name"] == "hello"

    @pytest.mark.asyncio
    async def test_empty_string_to_none(self, mock_db_function):
        """Test that empty string → None conversion still works."""

        @fraise_input
        class EmptyInput:
            name: str = fraise_field(normalize=["trim"])

        # Input: "   " (whitespace only)
        # Expected: None (empty after trim)

        # [Test execution]

        assert mock_db_function["data"]["name"] is None


class TestUnicodeNormalization:
    """Test unicode normalization."""

    @pytest.mark.asyncio
    async def test_nfc_normalization(self, mock_db_function):
        """Test NFC unicode normalization."""

        @fraise_input
        class UnicodeInput:
            name: str = fraise_field(normalize=["trim"])
            # [Need to add unicode_form parameter in Phase 1 extension]

        # Input: "café" (composed form)
        # Expected: "café" (NFC normalized)

        # [Test execution]
```

**Verification**:
```bash
# Run integration tests
uv run pytest tests/integration/test_mutation_normalization.py -v

# Expected: All tests pass
```

---

## Acceptance Criteria

- [ ] `fraise_field()` accepts `normalize` parameter
- [ ] `normalize=["trim", "lowercase"]` works for string fields
- [ ] `normalize=False` disables all normalization
- [ ] Default behavior is `trim` (backward compatible)
- [ ] Empty string → None conversion still works after normalization
- [ ] Unicode normalization (NFC, NFKC, NFD, NFKD) works
- [ ] Invalid normalization rules raise clear error messages
- [ ] All unit tests pass (>95% coverage)
- [ ] All integration tests pass
- [ ] No performance regression for non-normalized fields
- [ ] Existing mutations continue to work (backward compatible)

## Verification Commands

```bash
# Run all tests
uv run pytest tests/test_normalization.py tests/integration/test_mutation_normalization.py -v

# Run with coverage
uv run pytest tests/test_normalization.py --cov=fraiseql.normalization --cov-report=term-missing

# Test normalization functions directly
python -c "
from fraiseql.normalization import apply_normalization
print(apply_normalization('  HELLO WORLD  ', ['trim', 'lowercase']))
# Expected: hello world
"

# Test field metadata
python -c "
from fraiseql import fraise_field
field = fraise_field(normalize=['trim', 'lowercase'])
print('Normalization configured successfully')
"

# Test serialization
python -c "
from fraiseql.mutations.sql_generator import _serialize_value
result = _serialize_value('  HELLO  ')
assert result == 'HELLO', f'Expected HELLO (trimmed), got {result}'
print('Serialization works correctly')
"
```

## DO NOT

- ❌ Break backward compatibility (existing mutations must work)
- ❌ Apply normalization to non-string types (only strings)
- ❌ Change current trim behavior (keep as default)
- ❌ Add complex validation (save for Phase 4)
- ❌ Add database-specific normalization (must work across all databases)
- ❌ Over-engineer (keep it simple and focused)

## Notes

1. **Field metadata extraction**: May need to handle different field type representations (Annotated, dataclass fields, Strawberry fields)
2. **Performance**: Normalization should only run when explicitly configured (zero overhead for existing code)
3. **Error messages**: Should be clear and helpful (e.g., "Invalid normalization rule: 'lowrcase'. Did you mean 'lowercase'?")
4. **Unicode edge cases**: Test with emojis, RTL text, CJK characters, combining characters
5. **Compatibility**: Ensure normalization works with `prepare_input()` hook (applied before hook)

## Estimated Time

**4-6 hours** (including tests and verification)

- Step 1: Create normalization module (1 hour)
- Step 2: Extend `fraise_field()` (30 min)
- Step 3: Integrate into serialization (1 hour)
- Step 4: Update call sites (30 min)
- Step 5: Unit tests (1 hour)
- Step 6: Integration tests (1-2 hours)

## Next Phase

**Phase 2**: Type-Level Normalization Defaults
- Add `@fraise_input(normalize_strings=["trim", "lowercase"])` decorator parameter
- Allow type-wide normalization defaults
- Field-level overrides take precedence
