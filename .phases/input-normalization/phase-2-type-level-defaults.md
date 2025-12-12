# Phase 2: Type-Level Normalization Defaults

## Objective

Add type-level normalization configuration to `@fraise_input` decorator, allowing developers to specify default normalization rules for all string fields in an input type.

## Context

Phase 1 implemented field-level normalization:
```python
@fraise_input
class CreateUserInput:
    name: str = fraise_field(normalize=["trim", "capitalize"])
    email: str = fraise_field(normalize=["trim", "lowercase"])
    username: str = fraise_field(normalize=["trim", "lowercase"])
```

This is verbose when many fields need the same normalization. Type-level defaults reduce boilerplate:
```python
@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateUserInput:
    name: str = fraise_field(normalize=["capitalize"])  # Overrides: trim + capitalize
    email: str  # Inherits: trim + lowercase
    username: str  # Inherits: trim + lowercase
```

## Architecture

### Normalization Priority

1. **Field-level** (highest priority): `fraise_field(normalize=[...])`
2. **Type-level**: `@fraise_input(normalize_strings=[...])`
3. **Global** (Phase 3): `SchemaConfig.default_string_normalization`
4. **Framework default**: `["trim"]`

### Metadata Storage

Type-level normalization rules stored in class metadata:
```python
@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateUserInput:
    __normalization_config__ = {
        "normalize_strings": ["trim", "lowercase"],
        "unicode_form": None,  # Phase 3
    }
```

## Files to Modify

### 1. `src/fraiseql/types/fraise_input.py` (Extend `@fraise_input` decorator)
**Lines**: 27-96

**Changes**:
- Add `normalize_strings` parameter to decorator
- Add `unicode_form` parameter (optional unicode normalization)
- Store normalization config in class metadata
- Validate normalization rules

### 2. `src/fraiseql/mutations/sql_generator.py` (Update normalization logic)
**Lines**: Previously modified in Phase 1

**Changes**:
- Update `_get_field_metadata()` to check type-level defaults
- Update `_apply_normalization()` to use type-level defaults if no field-level rules
- Implement normalization priority resolution

### 3. `tests/test_type_normalization.py` (NEW FILE)
Unit tests for type-level normalization.

### 4. `tests/integration/test_type_level_normalization.py` (NEW FILE)
Integration tests with real mutations.

## Implementation Steps

### Step 1: Extend `@fraise_input` Decorator
**File**: `src/fraiseql/types/fraise_input.py`

**Current decorator** (line 27):
```python
def fraise_input(cls=None, *, description: str | None = None):
    """Decorator to mark a class as a FraiseQL input type."""
    ...
```

**New decorator**:
```python
def fraise_input(
    cls=None,
    *,
    description: str | None = None,
    normalize_strings: list[str] | Literal[False] | None = None,
    unicode_form: Literal["NFC", "NFKC", "NFD", "NFKD"] | None = None,
):
    """
    Decorator to mark a class as a FraiseQL input type.

    Args:
        cls: The class to decorate (auto-filled by Python)
        description: Description for GraphQL schema
        normalize_strings: Default normalization rules for all string fields
                          Set to False to disable default normalization
                          Valid rules: trim, lowercase, uppercase, capitalize, strip
        unicode_form: Unicode normalization form (NFC, NFKC, NFD, NFKD)

    Examples:
        >>> @fraise_input(normalize_strings=["trim", "lowercase"])
        >>> class CreateUserInput:
        >>>     email: str  # Will be trimmed and lowercased
        >>>     name: str = fraise_field(normalize=["capitalize"])  # Override

        >>> @fraise_input(normalize_strings=False)
        >>> class RawInput:
        >>>     data: str  # No normalization (not even trim)
    """
    from fraiseql.normalization import validate_normalization_rules

    # Validate normalization rules
    if normalize_strings is not None:
        validate_normalization_rules(normalize_strings)

    def decorator(cls):
        # Store normalization config in class metadata
        cls.__normalization_config__ = {
            "normalize_strings": normalize_strings,
            "unicode_form": unicode_form,
        }

        # Apply existing fraise_input logic
        # [Keep existing decorator logic unchanged]

        return cls

    # Handle both @fraise_input and @fraise_input(...) syntax
    if cls is None:
        return decorator
    else:
        return decorator(cls)
```

**Verification**:
```bash
python -c "
from fraiseql import fraise_input

@fraise_input(normalize_strings=['trim', 'lowercase'])
class TestInput:
    email: str

assert hasattr(TestInput, '__normalization_config__')
assert TestInput.__normalization_config__['normalize_strings'] == ['trim', 'lowercase']
print('OK')
"
```

---

### Step 2: Update Normalization Resolution Logic
**File**: `src/fraiseql/mutations/sql_generator.py`

**Update `_get_field_metadata()` function** (created in Phase 1):

```python
def _get_field_metadata(
    field_type: Any,
    field_name: str | None = None,
    input_class: type | None = None,
) -> dict[str, Any]:
    """
    Extract metadata from a field type, with fallback to type-level defaults.

    Args:
        field_type: The field type (from type annotations)
        field_name: Optional field name for debugging
        input_class: Optional input class (for type-level normalization)

    Returns:
        Field metadata dict with normalization rules resolved
    """
    # Get field-level metadata
    field_metadata = {}

    if hasattr(field_type, "metadata"):
        field_metadata = field_type.metadata or {}
    elif hasattr(field_type, "__metadata__"):
        for item in field_type.__metadata__:
            if isinstance(item, dict):
                field_metadata = item
                break

    # If no field-level normalization, check type-level defaults
    if "normalize" not in field_metadata and input_class:
        if hasattr(input_class, "__normalization_config__"):
            config = input_class.__normalization_config__
            type_level_normalize = config.get("normalize_strings")

            # Only apply type-level normalization to string fields
            # (This check happens in _apply_normalization, but we store it here)
            if type_level_normalize is not None:
                field_metadata["normalize"] = type_level_normalize

            # Store unicode form (if specified)
            unicode_form = config.get("unicode_form")
            if unicode_form:
                field_metadata["unicode_form"] = unicode_form

    return field_metadata
```

**Update `_apply_normalization()` function**:

```python
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
    from fraiseql.normalization import apply_normalization

    # Get normalization rules from metadata
    normalize_rules = field_metadata.get("normalize")

    # If normalize explicitly set to False, skip normalization
    if normalize_rules is False:
        return value

    # If no rules specified, apply default trim (current behavior)
    # This maintains backward compatibility
    if normalize_rules is None:
        normalize_rules = ["trim"]

    # Get unicode normalization form (optional)
    unicode_form = field_metadata.get("unicode_form")

    # Apply normalization
    return apply_normalization(value, normalize_rules, unicode_form=unicode_form)
```

**Update `_serialize_value()` to pass input class**:

```python
def _serialize_value(
    value: Any,
    field_type: Any = None,
    field_name: str | None = None,
    input_class: type | None = None,
) -> Any:
    """
    Serialize a value for JSONB storage.

    Args:
        value: Value to serialize
        field_type: Type annotation for the field (used to extract metadata)
        field_name: Optional field name for debugging
        input_class: Optional input class (for type-level normalization)

    Returns:
        Serialized value suitable for JSONB
    """
    if value is UNSET:
        return None

    if isinstance(value, str):
        # Get field metadata (with type-level fallback)
        field_metadata = _get_field_metadata(
            field_type,
            field_name,
            input_class=input_class
        )

        # Apply normalization
        result = _apply_normalization(value, field_metadata)

        # Convert empty string to None
        return None if result == "" else result

    # [Rest of function unchanged...]

    # Handle dataclasses and FraiseQL input objects
    if hasattr(value, "__dataclass_fields__") or hasattr(value, "__annotations__"):
        result = {}
        annotations = getattr(value, "__annotations__", {})

        # Get input class for type-level normalization
        value_class = type(value)

        for key, field_type in annotations.items():
            if hasattr(value, key):
                field_value = getattr(value, key)
                if field_value is not UNSET:
                    result[dict_key_to_snake_case(key)] = _serialize_value(
                        field_value,
                        field_type=field_type,
                        field_name=key,
                        input_class=value_class,  # Pass input class
                    )

        return result

    # [Rest unchanged...]
```

**Verification**:
```bash
python -c "
from fraiseql import fraise_input, fraise_field
from fraiseql.mutations.sql_generator import _serialize_value

@fraise_input(normalize_strings=['trim', 'lowercase'])
class TestInput:
    email: str
    name: str = fraise_field(normalize=['capitalize'])

# Simulate serialization
input_obj = TestInput(email='  USER@EXAMPLE.COM  ', name='  john doe  ')

# [Test serialization logic]
print('Type-level normalization configured')
"
```

---

### Step 3: Write Unit Tests
**File**: `tests/test_type_normalization.py`

```python
"""
Unit tests for type-level normalization.
"""

import pytest
from fraiseql import fraise_input, fraise_field


class TestTypeLevelNormalization:
    """Test type-level normalization configuration."""

    def test_type_level_decorator(self):
        """Test @fraise_input with normalize_strings parameter."""

        @fraise_input(normalize_strings=["trim", "lowercase"])
        class TestInput:
            email: str

        assert hasattr(TestInput, "__normalization_config__")
        config = TestInput.__normalization_config__
        assert config["normalize_strings"] == ["trim", "lowercase"]
        assert config["unicode_form"] is None

    def test_type_level_with_unicode(self):
        """Test type-level unicode normalization."""

        @fraise_input(
            normalize_strings=["trim"],
            unicode_form="NFC"
        )
        class TestInput:
            name: str

        config = TestInput.__normalization_config__
        assert config["normalize_strings"] == ["trim"]
        assert config["unicode_form"] == "NFC"

    def test_type_level_disabled(self):
        """Test normalize_strings=False disables normalization."""

        @fraise_input(normalize_strings=False)
        class TestInput:
            data: str

        config = TestInput.__normalization_config__
        assert config["normalize_strings"] is False

    def test_invalid_normalization_rule(self):
        """Test that invalid rules raise errors."""

        with pytest.raises(ValueError, match="Invalid normalization rule"):
            @fraise_input(normalize_strings=["invalid_rule"])
            class TestInput:
                data: str


class TestNormalizationPriority:
    """Test normalization priority resolution."""

    def test_field_overrides_type(self):
        """Test that field-level normalization overrides type-level."""

        @fraise_input(normalize_strings=["trim", "lowercase"])
        class TestInput:
            # Field-level should override type-level
            name: str = fraise_field(normalize=["capitalize"])
            # No field-level, should use type-level
            email: str

        # [Test serialization to verify priority]
        # Input: name="  JOHN DOE  ", email="  USER@EXAMPLE.COM  "
        # Expected: name="John Doe", email="user@example.com"

    def test_field_disable_overrides_type(self):
        """Test that field-level False overrides type-level."""

        @fraise_input(normalize_strings=["trim", "lowercase"])
        class TestInput:
            data: str = fraise_field(normalize=False)  # Explicitly disabled

        # [Test serialization]
        # Input: data="  RAW DATA  "
        # Expected: data="  RAW DATA  " (unchanged)


class TestBackwardCompatibility:
    """Test backward compatibility with Phase 1."""

    def test_no_type_level_config(self):
        """Test that inputs without type-level config still work."""

        @fraise_input
        class TestInput:
            name: str  # Should default to trim (Phase 1 behavior)

        # Should NOT have normalization config (or have None values)
        if hasattr(TestInput, "__normalization_config__"):
            config = TestInput.__normalization_config__
            assert config["normalize_strings"] is None

    def test_field_level_without_type_level(self):
        """Test that field-level normalization works without type-level."""

        @fraise_input
        class TestInput:
            name: str = fraise_field(normalize=["trim", "lowercase"])

        # [Test serialization - should work exactly like Phase 1]
```

**Verification**:
```bash
uv run pytest tests/test_type_normalization.py -v
```

---

### Step 4: Write Integration Tests
**File**: `tests/integration/test_type_level_normalization.py`

```python
"""
Integration tests for type-level normalization with real mutations.
"""

import pytest
from fraiseql import fraise_input, fraise_field, mutation, success, failure


@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateTagInput:
    """Input with type-level normalization."""
    tag: str  # Inherits type-level: trim + lowercase
    description: str  # Inherits type-level: trim + lowercase
    display_name: str = fraise_field(normalize=["capitalize"])  # Override
    raw_metadata: str = fraise_field(normalize=False)  # Opt-out


@success
class CreateTagSuccess:
    tag_id: str


@failure
class CreateTagError:
    pass


class TestTypeLevelNormalizationIntegration:
    """Test type-level normalization in real mutations."""

    @pytest.mark.asyncio
    async def test_type_level_inherited(self, mock_db_function):
        """Test that fields inherit type-level normalization."""

        @mutation(function="create_tag")
        class CreateTag:
            input: CreateTagInput
            success: CreateTagSuccess
            error: CreateTagError

        # Execute mutation
        # Input: tag="  PYTHON  ", description="  A PROGRAMMING LANGUAGE  "
        # Expected: tag="python", description="a programming language"

        # [Execute mutation and assert]
        assert mock_db_function["data"]["tag"] == "python"
        assert mock_db_function["data"]["description"] == "a programming language"

    @pytest.mark.asyncio
    async def test_field_override(self, mock_db_function):
        """Test that field-level normalization overrides type-level."""

        # Input: display_name="  python language  "
        # Type-level: trim + lowercase
        # Field-level: capitalize (overrides)
        # Expected: "Python Language" (capitalize only, no lowercase)

        # [Execute mutation and assert]
        assert mock_db_function["data"]["display_name"] == "Python Language"

    @pytest.mark.asyncio
    async def test_field_opt_out(self, mock_db_function):
        """Test that normalize=False opts out of type-level normalization."""

        # Input: raw_metadata="  { KEY: VALUE }  "
        # Type-level: trim + lowercase
        # Field-level: False (opt-out)
        # Expected: "  { KEY: VALUE }  " (unchanged)

        # [Execute mutation and assert]
        assert mock_db_function["data"]["raw_metadata"] == "  { KEY: VALUE }  "


class TestUnicodeNormalizationTypeLevel:
    """Test type-level unicode normalization."""

    @pytest.mark.asyncio
    async def test_type_level_unicode(self, mock_db_function):
        """Test unicode normalization at type level."""

        @fraise_input(
            normalize_strings=["trim"],
            unicode_form="NFC"
        )
        class UnicodeInput:
            name: str

        # Input: "café" (composed or decomposed)
        # Expected: "café" (NFC normalized)

        # [Execute mutation and assert unicode normalization]
```

**Verification**:
```bash
uv run pytest tests/integration/test_type_level_normalization.py -v
```

---

## Acceptance Criteria

- [ ] `@fraise_input(normalize_strings=[...])` parameter works
- [ ] Type-level normalization applies to all string fields by default
- [ ] Field-level `fraise_field(normalize=[...])` overrides type-level
- [ ] `normalize=False` at field level opts out of type-level normalization
- [ ] `unicode_form` parameter works at type level
- [ ] Normalization priority: field > type > default (trim)
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Backward compatible with Phase 1 (inputs without type-level config still work)
- [ ] Clear error messages for invalid configurations

## Verification Commands

```bash
# Run type-level normalization tests
uv run pytest tests/test_type_normalization.py -v

# Run integration tests
uv run pytest tests/integration/test_type_level_normalization.py -v

# Run all normalization tests (Phase 1 + Phase 2)
uv run pytest tests/test_normalization.py tests/test_type_normalization.py -v

# Test decorator configuration
python -c "
from fraiseql import fraise_input

@fraise_input(normalize_strings=['trim', 'lowercase'])
class TestInput:
    email: str

config = TestInput.__normalization_config__
assert config['normalize_strings'] == ['trim', 'lowercase']
print('OK: Type-level normalization configured')
"

# Test normalization priority
python -c "
from fraiseql import fraise_input, fraise_field

@fraise_input(normalize_strings=['lowercase'])
class TestInput:
    # Field overrides type
    name: str = fraise_field(normalize=['uppercase'])
    # Field inherits type
    email: str

# [Test serialization to verify priority]
print('OK: Normalization priority works')
"
```

## DO NOT

- ❌ Break backward compatibility (inputs without type-level config must work)
- ❌ Apply type-level normalization to non-string fields
- ❌ Override field-level `normalize=False` with type-level defaults
- ❌ Change Phase 1 behavior (field-level normalization must still work)
- ❌ Add complex validation (save for Phase 4)

## Notes

1. **Normalization priority**: Field-level always wins, even if False
2. **Unicode normalization**: Applied at type level, can be combined with string normalization
3. **Metadata storage**: Use `__normalization_config__` class attribute (consistent with other FraiseQL metadata)
4. **Error messages**: Should indicate whether error is from field-level or type-level configuration
5. **Performance**: No overhead for inputs without type-level normalization

## Estimated Time

**2-3 hours** (including tests and verification)

- Step 1: Extend `@fraise_input` decorator (30 min)
- Step 2: Update normalization resolution (1 hour)
- Step 3: Unit tests (30 min)
- Step 4: Integration tests (1 hour)

## Next Phase

**Phase 3**: Global Configuration
- Add `SchemaConfig.default_string_normalization` parameter
- Implement 3-tier priority: field > type > global
- Global unicode normalization configuration
