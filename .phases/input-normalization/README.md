# Input Normalization Feature - Implementation Plan

## Overview

This directory contains phased implementation plans for adding **built-in input normalization** to FraiseQL.

## Feature Summary

Add declarative, field-level input normalization to FraiseQL mutations and input types, supporting:

- **String normalization**: trim, lowercase, uppercase, capitalize
- **Unicode normalization**: NFC, NFKC, NFD, NFKD
- **Type-level defaults**: Apply normalization rules to all fields in an input type
- **Global configuration**: System-wide normalization defaults
- **Validation framework** (optional): min/max length, regex patterns, custom validators

## Architecture

### Normalization Priority Hierarchy

1. **Field-level** (highest priority): `fraise_field(normalize=[...])`
2. **Type-level**: `@fraise_input(normalize_strings=[...])`
3. **Global**: `SchemaConfig.default_string_normalization`
4. **Framework default**: Current behavior (trim only)

### Implementation Points

- **Field metadata**: Extend `fraise_field()` in `fields.py`
- **Serialization layer**: Extend `_serialize_value()` in `mutations/sql_generator.py`
- **Type decorator**: Extend `@fraise_input` in `types/fraise_input.py`
- **Global config**: Extend `SchemaConfig` in `config/schema_config.py`

## Phases

### Phase 1: Field-Level Normalization (Core) [4-6 hours]
**Status**: Not started
**File**: `phase-1-field-level-normalization.md`

Implement field-level normalization API and core string transformations.

**Deliverables**:
- Extend `fraise_field()` with `normalize` parameter
- Implement string normalization functions (trim, lowercase, uppercase, capitalize)
- Implement unicode normalization (NFC, NFKC, NFD, NFKD)
- Integrate into `_serialize_value()` serialization layer
- Unit tests for all normalization functions
- Integration tests with real mutations

### Phase 2: Type-Level Defaults [2-3 hours]
**Status**: Not started
**File**: `phase-2-type-level-defaults.md`

Add type-level normalization configuration to `@fraise_input`.

**Deliverables**:
- Extend `@fraise_input` decorator with `normalize_strings` parameter
- Implement type-level normalization metadata storage
- Implement field-level override logic (field > type)
- Integration tests with type-level defaults
- Documentation for type-level normalization

### Phase 3: Global Configuration [2 hours]
**Status**: Not started
**File**: `phase-3-global-configuration.md`

Add global normalization defaults to `SchemaConfig`.

**Deliverables**:
- Extend `SchemaConfig` with `default_string_normalization` parameter
- Implement normalization priority resolution (field > type > global)
- Configuration validation (invalid normalization rules)
- Integration tests with global configuration
- Documentation for global configuration

### Phase 4: Validation Framework (Optional) [2-3 hours]
**Status**: Not started
**File**: `phase-4-validation-framework.md`

Add validation support to `fraise_field()` for domain-specific constraints.

**Note**: Reduced scope - FraiseQL already has rich type validation via scalars (Email, PhoneNumber, etc.). This phase focuses on length constraints and regex patterns.

**Deliverables**:
- Extend `fraise_field()` with `validate` parameter
- Implement string validators (min/max length, regex)
- Custom validator support (callable functions)
- Integrate with GraphQL error handling
- Integration tests with validation errors
- Documentation for validation framework

### Phase 5: Documentation & Examples [2-3 hours]
**Status**: Not started
**File**: `phase-5-documentation.md`

Comprehensive documentation and real-world examples.

**Deliverables**:
- Update main documentation with normalization guide
- Add examples to `examples/` directory
- Add migration guide for existing projects
- Update changelog
- Update type stubs for IDE autocomplete

## Total Estimated Time

- **Core features** (Phases 1-3): 8-11 hours
- **Optional validation** (Phase 4): 2-3 hours (reduced scope)
- **Documentation** (Phase 5): 2-3 hours
- **Total**: 12-17 hours

## Dependencies

- **Phase 1**: No dependencies (can start immediately)
- **Phase 2**: Requires Phase 1 (field-level normalization functions)
- **Phase 3**: Requires Phase 2 (type-level normalization metadata)
- **Phase 4**: Requires Phase 1 (normalization framework)
- **Phase 5**: Requires Phases 1-3 (core features complete)

## Success Criteria

- ✅ Declarative normalization for 90% of common use cases
- ✅ Backward compatible with existing mutations
- ✅ Zero performance impact when normalization not used
- ✅ Clear, Pythonic API (`fraise_field(normalize=["trim", "lowercase"])`)
- ✅ Comprehensive documentation and examples
- ✅ Full test coverage (unit + integration)

## Design Principles

1. **Declarative over imperative**: Use field metadata, not hooks
2. **Sensible defaults**: trim by default (current behavior), opt-in for others
3. **Composable**: Multiple normalization rules can be combined
4. **Explicit**: Clear error messages for invalid configurations
5. **Performance**: Normalization only when configured (zero overhead otherwise)

## Example Usage (After Implementation)

### Field-Level Normalization
```python
from fraiseql import fraise_input, fraise_field

@fraise_input
class CreateUserInput:
    name: str = fraise_field(normalize=["trim", "capitalize"])
    email: str = fraise_field(normalize=["trim", "lowercase"], validate={"email": True})
    notes: str = fraise_field(normalize=["trim"])  # trim only
    code: str = fraise_field(normalize=["trim", "uppercase"])
```

### Type-Level Normalization
```python
@fraise_input(normalize_strings=["trim", "lowercase"])
class CreateTagInput:
    tag: str  # Auto-normalized: trim + lowercase
    description: str  # Auto-normalized: trim + lowercase
    metadata: str = fraise_field(normalize=False)  # Opt-out
```

### Global Configuration
```python
from fraiseql.config import SchemaConfig

SchemaConfig.set_config(
    default_string_normalization=["trim"],  # Global default
    unicode_normalization="NFC"  # Global unicode normalization
)
```

## Testing Strategy

### Unit Tests
- Test each normalization function in isolation
- Test normalization priority (field > type > global)
- Test edge cases (None, empty string, unicode, emojis)
- Test performance (normalization overhead)

### Integration Tests
- Test with real mutations (create, update)
- Test with nested input types
- Test with prepare_input() hook (ensure compatibility)
- Test with existing mutations (backward compatibility)

### Regression Tests
- Ensure existing mutations continue to work
- Ensure string trimming still works (current behavior)
- Ensure empty string → None conversion still works

## Migration Guide (For Existing Projects)

### Opt-In by Default
- **No changes required**: Existing mutations continue to work
- **Explicit trimming**: Current behavior (automatic trim) can be made explicit:
  ```python
  name: str = fraise_field(normalize=["trim"])  # Make current behavior explicit
  ```

### Migrating from prepare_input()
Before:
```python
@mutation
class CreateUser:
    input: CreateUserInput

    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        if "email" in input_data:
            input_data["email"] = input_data["email"].lower()
        return input_data
```

After:
```python
@fraise_input
class CreateUserInput:
    email: str = fraise_field(normalize=["trim", "lowercase"])
```

## Open Questions

1. **Should trim be opt-out or opt-in?**
   - Current behavior: automatic trim for all strings
   - Option A: Make trim explicit (breaking change)
   - Option B: Keep trim as default, allow opt-out with `normalize=False`
   - **Recommendation**: Option B (backward compatible)

2. **Should normalization happen before or after prepare_input()?**
   - Option A: Before (prepare_input can override)
   - Option B: After (normalization is final)
   - **Recommendation**: Before (more flexible)

3. **Should validation errors use GraphQL error format?**
   - **Recommendation**: Yes, integrate with GraphQL validation errors

4. **Should normalization apply to non-string types?**
   - Example: Normalize integers (remove leading zeros)?
   - **Recommendation**: No, focus on strings for now (can extend later)

## References

- **Current string trimming**: `src/fraiseql/mutations/sql_generator.py:46`
- **Field metadata**: `src/fraiseql/fields.py:13-180`
- **Input decorator**: `src/fraiseql/types/fraise_input.py:27-96`
- **Serialization layer**: `src/fraiseql/mutations/sql_generator.py:29-74`
- **Configuration**: `src/fraiseql/config/schema_config.py:9-36`
