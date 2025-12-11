# Phase 2: Fix Implementation

## 🎯 Solution Overview

**Fix the decorator to explicitly add auto-populated fields to `__gql_fields__`** after `define_fraiseql_type()` completes.

### Why This Works

1. ✅ Decorator knows exactly which fields it added
2. ✅ Can create proper `FraiseQLField` instances with metadata
3. ✅ No changes to core infrastructure needed
4. ✅ Backward compatible with existing code
5. ✅ Centralized in one location

---

## 📝 Implementation Plan

### File to Modify

**Primary**: `src/fraiseql/mutations/decorators.py`

**Lines to modify**: 94-118 (the `success` decorator's `wrap` function)

### Current Code

```python
def wrap(cls: T) -> T:
    from fraiseql.gql.schema_builder import SchemaRegistry
    from fraiseql.types.constructor import define_fraiseql_type
    from fraiseql.types.errors import Error

    # Auto-inject standard mutation fields if not already present
    annotations = getattr(cls, "__annotations__", {})

    if "status" not in annotations:
        annotations["status"] = str
        cls.status = "success"  # Default value
    if "message" not in annotations:
        annotations["message"] = str | None
        cls.message = None  # Default value
    if "errors" not in annotations:
        annotations["errors"] = list[Error] | None
        cls.errors = None  # Default value

    cls.__annotations__ = annotations

    patch_missing_field_types(cls)
    cls = define_fraiseql_type(cls, kind="output")  # ❌ Returns before fields added to __gql_fields__

    SchemaRegistry.get_instance().register_type(cls)

    _success_registry[cls.__name__] = cls
    _maybe_register_union(cls.__name__)
    return cls
```

### Fixed Code

```python
def wrap(cls: T) -> T:
    from fraiseql.gql.schema_builder import SchemaRegistry
    from fraiseql.types.constructor import define_fraiseql_type
    from fraiseql.types.errors import Error
    from fraiseql.fields import FraiseQLField  # ✅ Import FraiseQLField

    # Track which fields we're auto-injecting for later
    auto_injected_fields = []

    # Auto-inject standard mutation fields if not already present
    annotations = getattr(cls, "__annotations__", {})

    if "status" not in annotations:
        annotations["status"] = str
        cls.status = "success"  # Default value
        auto_injected_fields.append("status")  # ✅ Track
    if "message" not in annotations:
        annotations["message"] = str | None
        cls.message = None  # Default value
        auto_injected_fields.append("message")  # ✅ Track
    if "errors" not in annotations:
        annotations["errors"] = list[Error] | None
        cls.errors = None  # Default value
        auto_injected_fields.append("errors")  # ✅ Track

    cls.__annotations__ = annotations

    patch_missing_field_types(cls)
    cls = define_fraiseql_type(cls, kind="output")

    # ✅ NEW: Add auto-injected fields to __gql_fields__
    if auto_injected_fields:
        gql_fields = getattr(cls, "__gql_fields__", {})
        type_hints = getattr(cls, "__gql_type_hints__", {})

        for field_name in auto_injected_fields:
            if field_name not in gql_fields:  # Don't override if already exists
                field_type = type_hints.get(field_name)
                if field_type:
                    # Create FraiseQLField for auto-injected field
                    gql_fields[field_name] = FraiseQLField(
                        name=field_name,
                        field_type=field_type,
                        purpose="output",
                        description=_get_auto_field_description(field_name),
                        graphql_name=None,  # Use default camelCase conversion
                    )

        # Update the class's __gql_fields__
        cls.__gql_fields__ = gql_fields

    SchemaRegistry.get_instance().register_type(cls)

    _success_registry[cls.__name__] = cls
    _maybe_register_union(cls.__name__)
    return cls


def _get_auto_field_description(field_name: str) -> str:
    """Get description for auto-injected mutation fields."""
    descriptions = {
        "status": "Operation status (always 'success' for success types)",
        "message": "Human-readable message describing the operation result",
        "errors": "List of errors (always empty for success types)",
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

---

## 🔄 Apply Same Fix to `@failure` Decorator

The `@failure` decorator has similar logic and needs the same fix.

### Current `@failure` Code (lines ~120-145)

```python
def wrap(cls: T) -> T:
    from fraiseql.gql.schema_builder import SchemaRegistry
    from fraiseql.types.constructor import define_fraiseql_type
    from fraiseql.types.errors import Error

    # Auto-inject standard mutation fields if not already present
    annotations = getattr(cls, "__annotations__", {})

    if "status" not in annotations:
        annotations["status"] = str
        cls.status = "error"  # Default for failure
    if "message" not in annotations:
        annotations["message"] = str | None
        cls.message = None
    if "errors" not in annotations:
        annotations["errors"] = list[Error] | None
        cls.errors = None

    cls.__annotations__ = annotations

    patch_missing_field_types(cls)
    cls = define_fraiseql_type(cls, kind="output")

    SchemaRegistry.get_instance().register_type(cls)

    _failure_registry[cls.__name__] = cls
    _maybe_register_union(cls.__name__)
    return cls
```

### Fixed `@failure` Code

```python
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


def _get_auto_field_description_failure(field_name: str) -> str:
    """Get description for auto-injected failure fields."""
    descriptions = {
        "status": "Error status code (e.g., 'error', 'failed', 'blocked')",
        "message": "Human-readable error message",
        "errors": "List of detailed error information",
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

---

## 🧩 Additional Fields to Handle

The Rust response builder also adds `id` and `updatedFields`. These need to be handled differently.

### `id` Field (from `entity_id`)

**Rust code** (`response_builder.rs:100-103`):
```rust
if let Some(ref entity_id) = result.entity_id {
    obj.insert("id".to_string(), json!(entity_id));
}
```

**Problem**: `id` is conditionally added based on whether the mutation returns an entity.

**Solution**: Make `id` opt-in via decorator parameter or automatically detect if entity is returned.

```python
# Option 1: Auto-detect (recommended)
def wrap(cls: T) -> T:
    # ... existing code ...

    # Check if class has an entity field (common pattern: <entity>: SomeType)
    has_entity_field = any(
        field_name.lower() not in {"status", "message", "errors", "id", "updated_fields"}
        for field_name in type_hints.keys()
    )

    if has_entity_field and "id" not in annotations:
        annotations["id"] = str | None
        cls.id = None
        auto_injected_fields.append("id")

    # ... rest of code ...
```

### `updatedFields` Field (always present in Rust)

**Rust code** (`response_builder.rs:line unknown, assumed ~115`):
```rust
obj.insert("updatedFields".to_string(), json!(updated_fields));
```

**Solution**: Always add for success types (like status, message, errors).

```python
if "updated_fields" not in annotations:
    annotations["updated_fields"] = list[str] | None
    cls.updated_fields = None
    auto_injected_fields.append("updated_fields")
```

### Complete Enhanced Fix

```python
def wrap(cls: T) -> T:
    from fraiseql.gql.schema_builder import SchemaRegistry
    from fraiseql.types.constructor import define_fraiseql_type
    from fraiseql.types.errors import Error
    from fraiseql.fields import FraiseQLField

    auto_injected_fields = []
    annotations = getattr(cls, "__annotations__", {})

    # Always add status, message, errors
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

    # Add updatedFields (Rust always includes this)
    if "updated_fields" not in annotations:
        annotations["updated_fields"] = list[str] | None
        cls.updated_fields = None
        auto_injected_fields.append("updated_fields")

    # Conditionally add id if entity field detected
    has_entity_field = any(
        field_name not in {"status", "message", "errors", "id", "updated_fields"}
        for field_name in annotations.keys()
    )
    if has_entity_field and "id" not in annotations:
        annotations["id"] = str | None
        cls.id = None
        auto_injected_fields.append("id")

    cls.__annotations__ = annotations

    patch_missing_field_types(cls)
    cls = define_fraiseql_type(cls, kind="output")

    # Add auto-injected fields to __gql_fields__
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
                        description=_get_auto_field_description(field_name),
                        graphql_name=None,
                    )

        cls.__gql_fields__ = gql_fields

    SchemaRegistry.get_instance().register_type(cls)

    _success_registry[cls.__name__] = cls
    _maybe_register_union(cls.__name__)
    return cls


def _get_auto_field_description(field_name: str) -> str:
    """Get description for auto-injected mutation fields."""
    descriptions = {
        "status": "Operation status (always 'success' for success types)",
        "message": "Human-readable message describing the operation result",
        "errors": "List of errors (always empty for success types)",
        "id": "ID of the created/updated entity",
        "updated_fields": "List of field names that were updated",
    }
    return descriptions.get(field_name, f"Auto-populated {field_name} field")
```

---

## 🔬 Verification After Fix

### Before (Broken)

```python
@success
class CreateMachineSuccess:
    machine: Machine

# After decorator:
CreateMachineSuccess.__gql_fields__ = {
    "machine": FraiseQLField(...),
    # ❌ Missing: status, message, errors, id, updated_fields
}
```

### After (Fixed)

```python
@success
class CreateMachineSuccess:
    machine: Machine

# After decorator:
CreateMachineSuccess.__gql_fields__ = {
    "machine": FraiseQLField(...),
    "status": FraiseQLField(...),       # ✅ Added
    "message": FraiseQLField(...),      # ✅ Added
    "errors": FraiseQLField(...),       # ✅ Added
    "id": FraiseQLField(...),           # ✅ Added
    "updated_fields": FraiseQLField(...),  # ✅ Added
}
```

### GraphQL Schema Output

```graphql
type CreateMachineSuccess {
  machine: Machine!
  status: String!          # ✅ Now in schema
  message: String          # ✅ Now in schema
  errors: [Error!]         # ✅ Now in schema
  id: String               # ✅ Now in schema
  updatedFields: [String!] # ✅ Now in schema (camelCase)
}
```

---

## 🛡️ Edge Cases to Handle

### 1. User Explicitly Defines Auto-Populated Field

```python
@success
class CreateMachineSuccess:
    machine: Machine
    status: str = "custom_success"  # User wants custom status
```

**Fix**: Check if field already in `gql_fields` before adding.

```python
if field_name not in gql_fields:  # ✅ Don't override
    gql_fields[field_name] = FraiseQLField(...)
```

### 2. User Uses `field()` for Auto-Populated Field

```python
from fraiseql.fields import fraise_field

@success
class CreateMachineSuccess:
    machine: Machine
    message: str | None = fraise_field(description="Custom message description")
```

**Fix**: Same as above - `field_name not in gql_fields` check prevents override.

### 3. Decorator Applied to Class Without Entity Field

```python
@success
class DeleteSuccess:
    pass  # No entity field, just confirms deletion
```

**Fix**: Auto-detection should handle this:
```python
has_entity_field = any(
    field_name not in {"status", "message", "errors", "id", "updated_fields"}
    for field_name in annotations.keys()
)
# has_entity_field = False, so id is NOT added
```

---

## ✅ Implementation Checklist

- [ ] Modify `@success` decorator to track auto-injected fields
- [ ] Add fields to `__gql_fields__` after `define_fraiseql_type()`
- [ ] Modify `@failure` decorator with same logic
- [ ] Add helper function `_get_auto_field_description()`
- [ ] Handle `id` field conditionally based on entity detection
- [ ] Add `updated_fields` field unconditionally
- [ ] Add edge case checks (don't override existing fields)
- [ ] Write unit tests (see Phase 3)
- [ ] Verify GraphQL schema introspection includes new fields
- [ ] Test with PrintOptim backend (138 tests should pass)

---

**Next**: [Phase 3: Testing Strategy](./phase-3-testing.md)
