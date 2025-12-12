# Input Normalization - Quick Start Guide

## 📋 Implementation Checklist

### Phase 1: Field-Level Normalization (4-6 hours) ✅ RECOMMENDED
- [ ] Create `src/fraiseql/normalization.py` (normalization functions)
- [ ] Extend `fraise_field()` with `normalize` parameter
- [ ] Update `_serialize_value()` to apply normalization
- [ ] Write unit tests (`tests/test_normalization.py`)
- [ ] Write integration tests (`tests/integration/test_mutation_normalization.py`)

**Result**: `email: str = fraise_field(normalize=["trim", "lowercase"])`

---

### Phase 2: Type-Level Defaults (2-3 hours) ✅ RECOMMENDED
- [ ] Extend `@fraise_input` with `normalize_strings` parameter
- [ ] Update `_get_field_metadata()` to check type-level defaults
- [ ] Implement normalization priority (field > type)
- [ ] Write unit tests (`tests/test_type_normalization.py`)
- [ ] Write integration tests (`tests/integration/test_type_level_normalization.py`)

**Result**: `@fraise_input(normalize_strings=["trim", "lowercase"])`

---

### Phase 3: Global Configuration (2 hours) ✅ RECOMMENDED
- [ ] Extend `SchemaConfig` with global normalization parameters
- [ ] Update `_apply_normalization()` to use global defaults
- [ ] Implement 4-tier priority (field > type > global > default)
- [ ] Write unit tests (`tests/test_global_normalization.py`)
- [ ] Write integration tests (`tests/integration/test_global_config.py`)

**Result**: `SchemaConfig.set_config(default_string_normalization=["trim"])`

---

### Phase 4: Validation Framework (4-5 hours) ⚠️ OPTIONAL
- [ ] Create `src/fraiseql/validation.py` (validation functions)
- [ ] Extend `fraise_field()` with `validate` parameter (already stubbed)
- [ ] Apply validation in `_serialize_value()` (after normalization)
- [ ] Write unit tests (`tests/test_validation.py`)
- [ ] Write integration tests (`tests/integration/test_mutation_validation.py`)

**Result**: `email: str = fraise_field(validate={"email": True, "max_length": 255})`

---

### Phase 5: Documentation (2-3 hours) ✅ REQUIRED
- [ ] Write main documentation (`docs/features/input-normalization.md`)
- [ ] Update API reference (`docs/api/fields.md`)
- [ ] Create examples (`examples/normalization/*.py`)
- [ ] Write migration guide
- [ ] Update changelog (`CHANGELOG.md`)
- [ ] Update type stubs (`src/fraiseql/fields.pyi`)

**Result**: Comprehensive documentation and examples

---

## 🎯 Recommended Implementation Order

**Minimum Viable Feature** (8-11 hours):
1. Phase 1: Field-Level Normalization
2. Phase 2: Type-Level Defaults
3. Phase 3: Global Configuration
4. Phase 5: Documentation

**Full Feature** (14-19 hours):
1. Phase 1: Field-Level Normalization
2. Phase 2: Type-Level Defaults
3. Phase 3: Global Configuration
4. Phase 4: Validation Framework
5. Phase 5: Documentation

---

## 🚀 Quick Reference

### Field-Level Normalization
```python
from fraiseql import fraise_field

email: str = fraise_field(normalize=["trim", "lowercase"])
name: str = fraise_field(normalize=["trim", "capitalize"])
code: str = fraise_field(normalize=["uppercase"])
raw: str = fraise_field(normalize=False)  # Opt-out
```

### Type-Level Normalization
```python
from fraiseql import fraise_input, fraise_field

@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateTagInput:
    tag: str  # Inherits: trim + lowercase
    description: str  # Inherits: trim + lowercase
    display_name: str = fraise_field(normalize=["capitalize"])  # Override
```

### Global Configuration
```python
from fraiseql.config import SchemaConfig

SchemaConfig.set_config(
    default_string_normalization=["trim"],
    unicode_normalization="NFC"
)
```

### Validation (Optional)
```python
email: str = fraise_field(
    normalize=["trim", "lowercase"],
    validate={"email": True, "max_length": 255}
)

password: str = fraise_field(
    validate={
        "min_length": 8,
        "regex": r"^(?=.*[A-Z])(?=.*\d)"
    }
)
```

---

## 📚 Available Normalizers

| Rule | Description | Example |
|------|-------------|---------|
| `trim` | Remove whitespace | `"  hello  "` → `"hello"` |
| `lowercase` | Convert to lowercase | `"HELLO"` → `"hello"` |
| `uppercase` | Convert to uppercase | `"hello"` → `"HELLO"` |
| `capitalize` | Capitalize words | `"hello world"` → `"Hello World"` |

## 📚 Unicode Normalization Forms

| Form | Description |
|------|-------------|
| `NFC` | Canonical Decomposition + Composition (most common) |
| `NFD` | Canonical Decomposition |
| `NFKC` | Compatibility Decomposition + Composition |
| `NFKD` | Compatibility Decomposition |

---

## 🎯 Normalization Priority

1. **Field-level** (highest): `fraise_field(normalize=[...])`
2. **Type-level**: `@fraise_input(normalize_strings=[...])`
3. **Global**: `SchemaConfig.default_string_normalization`
4. **Framework default** (lowest): `["trim"]`

---

## ✅ Acceptance Criteria Summary

### Phase 1
- [ ] Field-level normalization API works
- [ ] All normalizers implemented (trim, lowercase, uppercase, capitalize)
- [ ] Unicode normalization works (NFC, NFKC, NFD, NFKD)
- [ ] Backward compatible (default trim behavior preserved)
- [ ] All tests pass (>95% coverage)

### Phase 2
- [ ] Type-level normalization API works
- [ ] Field-level overrides type-level
- [ ] Inheritance works correctly
- [ ] All tests pass

### Phase 3
- [ ] Global configuration API works
- [ ] 4-tier priority resolution correct (field > type > global > default)
- [ ] All tests pass

### Phase 4 (Optional)
- [ ] Validation API works
- [ ] String and numeric validators implemented
- [ ] Custom validators supported
- [ ] GraphQL error integration works
- [ ] All tests pass

### Phase 5
- [ ] Documentation complete
- [ ] Examples runnable and tested
- [ ] Migration guide clear
- [ ] Type stubs updated

---

## 📖 Documentation Files

| File | Purpose |
|------|---------|
| `README.md` | Overview and phase summary |
| `ASSESSMENT.md` | Detailed assessment and recommendation |
| `QUICK_START.md` | This file (quick reference) |
| `phase-1-field-level-normalization.md` | Detailed Phase 1 plan |
| `phase-2-type-level-defaults.md` | Detailed Phase 2 plan |
| `phase-3-global-configuration.md` | Detailed Phase 3 plan |
| `phase-4-validation-framework.md` | Detailed Phase 4 plan (optional) |
| `phase-5-documentation.md` | Detailed Phase 5 plan |

---

## 🔧 Testing Commands

```bash
# Run all normalization tests
uv run pytest tests/test_normalization.py tests/test_type_normalization.py tests/test_global_normalization.py -v

# Run integration tests
uv run pytest tests/integration/test_mutation_normalization.py -v

# Run with coverage
uv run pytest tests/test_normalization.py --cov=fraiseql.normalization --cov-report=term-missing

# Test specific normalizer
python -c "from fraiseql.normalization import apply_normalization; print(apply_normalization('  HELLO  ', ['trim', 'lowercase']))"
```

---

## 🐛 Common Issues & Solutions

### Issue: Normalization not applied
**Solution**: Check that field has `normalize` parameter:
```python
# ❌ Wrong
email: str

# ✅ Correct
email: str = fraise_field(normalize=["trim", "lowercase"])
```

### Issue: Type-level normalization not inherited
**Solution**: Ensure `@fraise_input` decorator has `normalize_strings`:
```python
# ❌ Wrong
@fraise_input
class Input:
    email: str

# ✅ Correct
@fraise_input(normalize_strings=["trim", "lowercase"])
class Input:
    email: str
```

### Issue: Field override not working
**Solution**: Field-level must be explicit (not None):
```python
# ❌ Wrong (inherits type-level)
email: str = fraise_field()

# ✅ Correct (overrides type-level)
email: str = fraise_field(normalize=["uppercase"])
```

---

## 📞 Support

- **Feature Request**: Original request in project root
- **Architecture**: See `ASSESSMENT.md`
- **Detailed Plans**: See individual phase files
- **Questions**: Consult FraiseQL maintainers

---

**Last Updated**: 2025-12-11
