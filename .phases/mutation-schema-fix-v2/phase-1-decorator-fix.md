# Phase 1: Python Decorator Fix (RED → GREEN)

## 🎯 Objective

Modify `@success` and `@failure` decorators to add auto-populated fields to `__gql_fields__` so they appear in GraphQL schema.

**Time**: 1.5 hours

---

## 📋 Context

### The Bug
Decorator adds fields to `cls.__annotations__` but NOT to `cls.__gql_fields__`:

```python
# Current (BROKEN):
@success
class CreateMachineSuccess:
    machine: Machine

# After decorator:
CreateMachineSuccess.__annotations__ = {
    "machine": Machine,
    "status": str,           # ✅ Added
    "message": str | None,   # ✅ Added
    "errors": list[Error],   # ✅ Added
}

CreateMachineSuccess.__gql_fields__ = {
    "machine": FraiseQLField(...),
    # ❌ Missing: status, message, errors
}
```

Schema generator reads ONLY `__gql_fields__`, so fields are invisible to GraphQL.

---

## 🔧 The Fix

Add fields to `__gql_fields__` explicitly after `define_fraiseql_type()` completes.

### File to Modify
`src/fraiseql/mutations/decorators.py`

---

## 📝 Implementation Steps

### Step 1: Write Failing Test (RED) - 15 min

**Create**: `tests/unit/mutations/test_auto_populate_schema.py`

```python
"""Test that auto-populated fields appear in GraphQL schema."""
import pytest
from fraiseql.mutations.decorators import success, failure
from fraiseql.decorators import fraise_type


@fraise_type
class Machine:
    id: str
    name: str


def test_success_decorator_adds_fields_to_gql_fields():
    """Auto-populated fields should be in __gql_fields__ for schema generation."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # All auto-populated fields must be present
    assert "machine" in gql_fields, "Original field should be present"
    assert "status" in gql_fields, "Auto-injected status missing"
    assert "message" in gql_fields, "Auto-injected message missing"
    assert "errors" in gql_fields, "Auto-injected errors missing"
    assert "updated_fields" in gql_fields, "Auto-injected updatedFields missing"
    assert "id" in gql_fields, "Auto-injected id missing (entity detected)"

    # Verify field types
    assert gql_fields["status"].field_type == str
    assert gql_fields["message"].field_type == str | None


def test_failure_decorator_adds_fields():
    """Failure types should also get auto-populated fields."""

    @failure
    class CreateMachineError:
        error_code: str

    gql_fields = getattr(CreateMachineError, "__gql_fields__", {})

    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "updated_fields" in gql_fields
    # No entity field, so id should NOT be added
    assert "id" not in gql_fields


def test_no_entity_field_no_id():
    """ID should not be added when no entity field present."""

    @success
    class DeleteSuccess:
        """Deletion confirmation without entity."""
        pass

    gql_fields = getattr(DeleteSuccess, "__gql_fields__", {})

    # Standard fields should be present
    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "updated_fields" in gql_fields

    # But NOT id (no entity field detected)
    assert "id" not in gql_fields


def test_user_defined_fields_not_overridden():
    """User's explicit field definitions should be preserved."""

    @success
    class CreateMachineSuccess:
        machine: Machine
        status: str = "custom_success"

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # User-defined status should be preserved
    assert "status" in gql_fields
    # But auto-injected fields should still be added
    assert "message" in gql_fields
    assert "errors" in gql_fields
```

**Run test** (should FAIL):
```bash
pytest tests/unit/mutations/test_auto_populate_schema.py -xvs
```

---

### Step 2: Implement Decorator Fix (GREEN) - 45 min

**File**: `src/fraiseql/mutations/decorators.py`

#### Modify `@success` decorator (lines ~94-118)

```python
def success(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation success type."""

    def wrap(cls: T) -> T:
        from fraiseql.gql.schema_builder import SchemaRegistry
        from fraiseql.types.constructor import define_fraiseql_type
        from fraiseql.types.errors import Error
        from fraiseql.fields import FraiseQLField  # ✅ Import

        # Track which fields we're auto-injecting
        auto_injected_fields = []

        # Auto-inject standard mutation fields if not already present
        annotations = getattr(cls, "__annotations__", {})

        if "status" not in annotations:
            annotations["status"] = str
            cls.status = "success"
            auto_injected_fields.append("status")

        if "message" not in annotations:
            annotations["message"] = str | None
            cls.message = None
            auto_injected_fields.append("message")

        if "errors" not in annotations:
            annotations["errors"] = list[Error] | None
            cls.errors = None
            auto_injected_fields.append("errors")

        # ✅ NEW: Add updatedFields (per CTO feedback)
        if "updated_fields" not in annotations:
            annotations["updated_fields"] = list[str] | None
            cls.updated_fields = None
            auto_injected_fields.append("updated_fields")

        cls.__annotations__ = annotations

        patch_missing_field_types(cls)
        cls = define_fraiseql_type(cls, kind="output")

        # ✅ NEW: Add auto-injected fields to __gql_fields__
        if auto_injected_fields:
            gql_fields = getattr(cls, "__gql_fields__", {})
            type_hints = getattr(cls, "__gql_type_hints__", {})

            for field_name in auto_injected_fields:
                # Don't override if user defined it explicitly
                if field_name not in gql_fields:
                    field_type = type_hints.get(field_name)
                    if field_type:
                        gql_fields[field_name] = FraiseQLField(
                            name=field_name,
                            field_type=field_type,
                            purpose="output",
                            description=_get_auto_field_description(field_name),
                            graphql_name=None,  # Use default camelCase
                        )

            cls.__gql_fields__ = gql_fields

        SchemaRegistry.get_instance().register_type(cls)

        _success_registry[cls.__name__] = cls
        _maybe_register_union(cls.__name__)
        return cls

    return wrap if _cls is None else wrap(_cls)


def _get_auto_field_description(field_name: str) -> str:
    """Get description for auto-injected mutation fields."""
    descriptions = {
        "status": "Operation status (always 'success' for success types)",
        "message": "Human-readable message describing the operation result",
        "errors": "List of errors (always empty for success types)",
        "updated_fields": "List of field names that were updated in the mutation",
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

#### Apply same fix to `@failure` decorator (lines ~120-145)

```python
def failure(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation failure type."""

    def wrap(cls: T) -> T:
        from fraiseql.gql.schema_builder import SchemaRegistry
        from fraiseql.types.constructor import define_fraiseql_type
        from fraiseql.types.errors import Error
        from fraiseql.fields import FraiseQLField  # ✅ Import

        # Track auto-injected fields
        auto_injected_fields = []

        annotations = getattr(cls, "__annotations__", {})

        if "status" not in annotations:
            annotations["status"] = str
            cls.status = "error"  # Default for failure
            auto_injected_fields.append("status")

        if "message" not in annotations:
            annotations["message"] = str | None
            cls.message = None
            auto_injected_fields.append("message")

        if "errors" not in annotations:
            annotations["errors"] = list[Error] | None
            cls.errors = None
            auto_injected_fields.append("errors")

        # ✅ NEW: Add updatedFields
        if "updated_fields" not in annotations:
            annotations["updated_fields"] = list[str] | None
            cls.updated_fields = None
            auto_injected_fields.append("updated_fields")

        cls.__annotations__ = annotations

        patch_missing_field_types(cls)
        cls = define_fraiseql_type(cls, kind="output")

        # ✅ NEW: Add auto-injected fields to __gql_fields__
        if auto_injected_fields:
            gql_fields = getattr(cls, "__gql_fields__", {})
            type_hints = getattr(cls, "__gql_type_hints__", {})

            for field_name in auto_injected_fields:
                if field_name not in gql_fields:
                    field_type = type_hints.get(field_name)
                    if field_type:
                        gql_fields[field_name] = FraiseQLField(
                            name=field_name,
                            field_type=field_type,
                            purpose="output",
                            description=_get_auto_field_description_failure(field_name),
                            graphql_name=None,
                        )

            cls.__gql_fields__ = gql_fields

        SchemaRegistry.get_instance().register_type(cls)

        _failure_registry[cls.__name__] = cls
        _maybe_register_union(cls.__name__)
        return cls

    return wrap if _cls is None else wrap(_cls)


def _get_auto_field_description_failure(field_name: str) -> str:
    """Get description for auto-injected failure fields."""
    descriptions = {
        "status": "Error status code (e.g., 'error', 'failed', 'blocked')",
        "message": "Human-readable error message",
        "errors": "List of detailed error information",
        "updated_fields": "List of field names that would have been updated",
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

**Run tests** (should PASS):
```bash
pytest tests/unit/mutations/test_auto_populate_schema.py -xvs
```

---

### Step 3: Add `id` Field Handling (Optional - 15 min)

The Rust response builder adds `id` from `entity_id` when an entity is present. We should add this to schema too.

**Detection logic**: Add `id` if there's a field that's not a standard auto-field.

```python
# In both @success and @failure decorators, BEFORE patch_missing_field_types():

# Detect if class has an entity field (any field that's not an auto-field)
has_entity_field = any(
    field_name not in {"status", "message", "errors", "updated_fields", "id"}
    for field_name in annotations.keys()
)

if has_entity_field and "id" not in annotations:
    annotations["id"] = str | None
    cls.id = None
    auto_injected_fields.append("id")
```

**Update description helper**:
```python
def _get_auto_field_description(field_name: str) -> str:
    descriptions = {
        "status": "Operation status (always 'success' for success types)",
        "message": "Human-readable message describing the operation result",
        "errors": "List of errors (always empty for success types)",
        "updated_fields": "List of field names that were updated in the mutation",
        "id": "ID of the created or updated entity",  # ✅ NEW
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

**Run tests again**:
```bash
pytest tests/unit/mutations/test_auto_populate_schema.py -xvs
```

---

### Step 4: Quick Sanity Check (15 min)

Run existing FraiseQL tests to ensure nothing broke:

```bash
# Unit tests
pytest tests/unit/mutations/ -xvs

# All unit tests
pytest tests/unit/ -v
```

If any tests fail, review and fix before proceeding.

---

## ✅ Acceptance Criteria

- [ ] All unit tests pass
- [ ] `@success` decorator adds fields to `__gql_fields__`
- [ ] `@failure` decorator adds fields to `__gql_fields__`
- [ ] `updatedFields` included in auto-injected list
- [ ] `id` conditionally added based on entity detection
- [ ] User-defined fields not overridden
- [ ] No regressions in existing tests

---

## 🔍 Verification Commands

```bash
# Run tests
pytest tests/unit/mutations/test_auto_populate_schema.py -xvs

# Check field registration manually
python3 -c "
from fraiseql.mutations.decorators import success
from fraiseql.decorators import fraise_type

@fraise_type
class Machine:
    id: str

@success
class CreateMachineSuccess:
    machine: Machine

print('__gql_fields__:', list(CreateMachineSuccess.__gql_fields__.keys()))
# Should print: ['machine', 'status', 'message', 'errors', 'updated_fields', 'id']
"
```

---

## 🚫 DO NOT

- ❌ Modify schema generator (`core/graphql_type.py`)
- ❌ Modify `define_fraiseql_type()` (`types/constructor.py`)
- ❌ Change Rust code (`fraiseql_rs/`)
- ❌ Add backward compatibility layers
- ❌ Add feature flags

---

**Next**: [Phase 2: Integration & Verification](./phase-2-integration-verification.md)
