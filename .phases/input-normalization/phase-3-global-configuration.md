# Phase 3: Global Configuration

## Objective

Add global normalization defaults to `SchemaConfig`, allowing project-wide normalization policies.

## Context

After Phases 1-2, normalization can be configured at field and type levels:
- **Field**: `fraise_field(normalize=["trim", "lowercase"])`
- **Type**: `@fraise_input(normalize_strings=["trim"])`

Phase 3 adds **global defaults** for the entire FraiseQL schema:
```python
from fraiseql.config import SchemaConfig

SchemaConfig.set_config(
    default_string_normalization=["trim"],
    unicode_normalization="NFC"
)
```

## Architecture

### Normalization Priority (Final)

1. **Field-level** (highest): `fraise_field(normalize=[...])`
2. **Type-level**: `@fraise_input(normalize_strings=[...])`
3. **Global** (new): `SchemaConfig.default_string_normalization`
4. **Framework default** (lowest): `["trim"]`

## Files to Modify

1. **`src/fraiseql/config/schema_config.py`** - Add global normalization config
2. **`src/fraiseql/mutations/sql_generator.py`** - Use global defaults in `_apply_normalization()`
3. **`tests/test_global_normalization.py`** - Unit tests
4. **`tests/integration/test_global_config.py`** - Integration tests

## Implementation Steps

### Step 1: Extend `SchemaConfig`
**File**: `src/fraiseql/config/schema_config.py`

```python
@dataclass
class SchemaConfig:
    camel_case_fields: bool = True
    # NEW: Global normalization defaults
    default_string_normalization: list[str] | Literal[False] | None = None
    unicode_normalization: Literal["NFC", "NFKC", "NFD", "NFKD"] | None = None

    def __post_init__(self):
        """Validate configuration."""
        from fraiseql.normalization import validate_normalization_rules

        if self.default_string_normalization is not None:
            validate_normalization_rules(self.default_string_normalization)
```

**Add global config accessor**:
```python
# Global config instance
_global_config: SchemaConfig | None = None


def get_global_config() -> SchemaConfig:
    """Get global FraiseQL configuration."""
    global _global_config
    if _global_config is None:
        _global_config = SchemaConfig()
    return _global_config


def set_global_config(**kwargs) -> None:
    """Set global FraiseQL configuration."""
    global _global_config
    _global_config = SchemaConfig(**kwargs)
```

### Step 2: Update Normalization Resolution
**File**: `src/fraiseql/mutations/sql_generator.py`

```python
def _apply_normalization(
    value: str,
    field_metadata: dict[str, Any],
) -> str:
    """Apply normalization with 4-tier priority: field > type > global > default."""
    from fraiseql.normalization import apply_normalization
    from fraiseql.config import get_global_config

    # Get normalization rules (field or type level)
    normalize_rules = field_metadata.get("normalize")

    # Priority 1: Field-level explicit False
    if normalize_rules is False:
        return value

    # Priority 2: Field or type-level rules
    if normalize_rules is not None:
        unicode_form = field_metadata.get("unicode_form")
        return apply_normalization(value, normalize_rules, unicode_form=unicode_form)

    # Priority 3: Global config
    global_config = get_global_config()
    if global_config.default_string_normalization is not None:
        return apply_normalization(
            value,
            global_config.default_string_normalization,
            unicode_form=global_config.unicode_normalization
        )

    # Priority 4: Framework default (trim)
    return apply_normalization(value, ["trim"])
```

### Step 3: Write Tests
**File**: `tests/test_global_normalization.py`

```python
import pytest
from fraiseql.config import set_global_config, get_global_config, SchemaConfig


class TestGlobalConfiguration:
    """Test global normalization configuration."""

    def teardown_method(self):
        """Reset global config after each test."""
        set_global_config()  # Reset to defaults

    def test_set_global_config(self):
        """Test setting global normalization config."""
        set_global_config(
            default_string_normalization=["trim", "lowercase"],
            unicode_normalization="NFC"
        )

        config = get_global_config()
        assert config.default_string_normalization == ["trim", "lowercase"]
        assert config.unicode_normalization == "NFC"

    def test_invalid_global_config(self):
        """Test that invalid global config raises errors."""
        with pytest.raises(ValueError, match="Invalid normalization rule"):
            set_global_config(default_string_normalization=["invalid_rule"])

    def test_global_config_disabled(self):
        """Test default_string_normalization=False disables global normalization."""
        set_global_config(default_string_normalization=False)

        config = get_global_config()
        assert config.default_string_normalization is False


class TestNormalizationPriorityWithGlobal:
    """Test 4-tier normalization priority."""

    def test_field_overrides_global(self):
        """Field-level normalization should override global config."""
        # Set global: lowercase
        # Set field: uppercase
        # Expected: uppercase (field wins)

    def test_type_overrides_global(self):
        """Type-level normalization should override global config."""
        # Set global: lowercase
        # Set type: uppercase
        # Expected: uppercase (type wins)

    def test_global_fallback(self):
        """Global config should be used when no field/type config."""
        # Set global: lowercase
        # No field/type config
        # Expected: lowercase (global used)

    def test_framework_default_fallback(self):
        """Framework default (trim) should be used when no config at all."""
        # No global, no type, no field config
        # Expected: trim (framework default)
```

## Acceptance Criteria

- [ ] `SchemaConfig.default_string_normalization` parameter works
- [ ] `SchemaConfig.unicode_normalization` parameter works
- [ ] Global config is used when no field/type normalization specified
- [ ] Field-level and type-level normalization override global config
- [ ] `default_string_normalization=False` disables global normalization
- [ ] Invalid global config raises clear errors
- [ ] All tests pass
- [ ] Backward compatible (no global config = trim by default)

## Verification Commands

```bash
# Test global config
python -c "
from fraiseql.config import set_global_config, get_global_config

set_global_config(
    default_string_normalization=['trim', 'lowercase'],
    unicode_normalization='NFC'
)

config = get_global_config()
assert config.default_string_normalization == ['trim', 'lowercase']
print('OK: Global config works')
"

# Run tests
uv run pytest tests/test_global_normalization.py -v
```

## Estimated Time

**2 hours**

## Next Phase

**Phase 4**: Validation Framework (optional)
