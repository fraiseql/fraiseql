# Phase 1: Root Cause Analysis

## 🔬 Deep Architectural Investigation

### The Bug in Three Sentences

1. The `@success` decorator adds fields to `cls.__annotations__` but NOT to `cls.__gql_fields__`
2. The schema generator reads ONLY `cls.__gql_fields__`, not `__annotations__`
3. Result: Fields exist in Python but are invisible to GraphQL schema

---

## 📊 Architecture Flow Diagram

### Current (Broken) Flow

```
User defines class:
┌─────────────────────────────────┐
│ @success                        │
│ class CreateMachineSuccess:     │
│     machine: Machine            │
└─────────────────────────────────┘
                ↓
Decorator runs (decorators.py:94-107):
┌─────────────────────────────────────────────────────┐
│ annotations = cls.__annotations__                   │
│ annotations["status"] = str          # ✅ Added     │
│ annotations["message"] = str | None  # ✅ Added     │
│ annotations["errors"] = list[Error]  # ✅ Added     │
│ cls.__annotations__ = annotations                   │
└─────────────────────────────────────────────────────┘
                ↓
define_fraiseql_type() called (constructor.py:161):
┌────────────────────────────────────────────────────┐
│ type_hints = get_type_hints(cls)  # Has all fields │
│ field_map = collect_fraise_fields(cls, type_hints) │
│                                                     │
│ BUT collect_fraise_fields() only creates           │
│ FraiseQLField objects for fields that had          │
│ explicit field() or fraise_field() calls!          │
│                                                     │
│ Result: field_map = {"machine": FraiseQLField(...)}│
│         (status, message, errors MISSING!)         │
└────────────────────────────────────────────────────┘
                ↓
Set metadata (constructor.py:174):
┌──────────────────────────────────────────────────┐
│ cls.__gql_fields__ = field_map                   │
│                                                   │
│ ❌ Missing: status, message, errors              │
└──────────────────────────────────────────────────┘
                ↓
Schema generation (graphql_type.py:427-433):
┌──────────────────────────────────────────────────┐
│ fields = getattr(typ, "__gql_fields__", {})      │
│ # Only has: {"machine": FraiseQLField(...)}      │
│                                                   │
│ for name, field in fields.items():               │
│     # Only iterates over "machine"               │
│     gql_fields[name] = GraphQLField(...)         │
│                                                   │
│ ❌ status, message, errors never added to schema │
└──────────────────────────────────────────────────┘
                ↓
GraphQL Schema:
┌──────────────────────────────────────────────────┐
│ type CreateMachineSuccess {                      │
│   machine: Machine  # ✅ In schema               │
│   # ❌ status, message, errors NOT in schema     │
│ }                                                 │
└──────────────────────────────────────────────────┘
                ↓
Rust response builder (response_builder.rs:100-112):
┌──────────────────────────────────────────────────┐
│ obj.insert("id", json!(entity_id));              │
│ obj.insert("message", json!(result.message));    │
│ obj.insert("status", json!(result.status));      │
│ obj.insert("errors", json!([]));                 │
│                                                   │
│ ✅ All fields added to runtime response          │
└──────────────────────────────────────────────────┘
                ↓
Result: Fields in response but NOT in schema! ❌
```

---

## 🔍 Code-Level Analysis

### 1. Decorator Adds to `__annotations__` Only

**File**: `src/fraiseql/mutations/decorators.py:94-107`

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

    cls.__annotations__ = annotations  # ✅ Updated

    patch_missing_field_types(cls)
    cls = define_fraiseql_type(cls, kind="output")  # ⚠️ Called AFTER annotations modified

    SchemaRegistry.get_instance().register_type(cls)

    _success_registry[cls.__name__] = cls
    _maybe_register_union(cls.__name__)
    return cls
```

**Problem**: `cls.__annotations__` updated, but `cls.__gql_fields__` is created LATER by `define_fraiseql_type()`.

---

### 2. `define_fraiseql_type` Calls `collect_fraise_fields`

**File**: `src/fraiseql/types/constructor.py:161-175`

```python
def define_fraiseql_type(
    cls: type[T],
    kind: Literal["input", "output", "type", "interface"],
) -> type[T]:
    """Core logic to define a FraiseQL input or output type."""
    typed_cls = cast("type[Any]", cls)

    # Get type hints (includes decorator-added fields)
    try:
        type_hints = get_type_hints(cls, localns={cls.__name__: cls}, include_extras=True)
        # ✅ type_hints DOES include status, message, errors
    except NameError as e:
        # ... error handling

    # BUT: collect_fraise_fields only creates FraiseQLField for explicitly defined fields
    field_map, patched_annotations = collect_fraise_fields(typed_cls, type_hints, kind=kind)
    # ❌ field_map does NOT include status, message, errors

    typed_cls.__annotations__ = patched_annotations
    typed_cls.__init__ = make_init(field_map, kw_only=True, type_kind=kind)

    # Set FraiseQL runtime metadata
    typed_cls.__gql_typename__ = typed_cls.__name__
    typed_cls.__gql_fields__ = field_map  # ❌ Missing decorator-added fields
    typed_cls.__gql_type_hints__ = type_hints  # ✅ Has decorator-added fields

    # ... rest of the function
```

**Problem**: `field_map` only contains fields that were explicitly defined with `field()` or `fraise_field()`. Decorator-added fields with simple assignments (`cls.status = "success"`) don't get `FraiseQLField` objects created.

---

### 3. `collect_fraise_fields` Logic

**File**: `src/fraiseql/utils/fraiseql_builder.py` (assumed location)

```python
def collect_fraise_fields(cls, type_hints, kind):
    """Collect fields that have FraiseQLField metadata."""
    field_map = {}

    for name, hint in type_hints.items():
        # Check if field has explicit field() definition
        if hasattr(cls, name):
            attr = getattr(cls, name)
            if isinstance(attr, FraiseQLField):
                field_map[name] = attr
            elif isinstance(attr, field):  # dataclass field
                # Convert to FraiseQLField
                field_map[name] = FraiseQLField(...)
            # ❌ ELSE: Plain attribute (like cls.status = "success") is IGNORED

    return field_map, patched_annotations
```

**Problem**: Fields added by decorator are plain attributes, not `FraiseQLField` instances, so they're skipped.

---

### 4. Schema Generator Only Reads `__gql_fields__`

**File**: `src/fraiseql/core/graphql_type.py:427-433`

```python
# Handle FraiseQL object-like types
if hasattr(typ, "__fraiseql_definition__"):
    definition = typ.__fraiseql_definition__
    if definition.kind in {"type", "success", "failure", "output"}:
        # Use the already collected fields from the decorator
        fields = getattr(typ, "__gql_fields__", {})  # ❌ Missing status, message, errors
        type_hints = getattr(typ, "__gql_type_hints__", {})  # ✅ Has all fields

        gql_fields = {}
        for name, field in fields.items():  # ❌ Only loops over explicit fields
            field_type = field.field_type or type_hints.get(name)
            if field_type is not None:
                # ... create GraphQL field
                gql_fields[graphql_field_name] = GraphQLField(...)

        # Result: gql_fields missing status, message, errors
        gql_type = GraphQLObjectType(
            name=typ.__name__,
            fields=gql_fields,  # ❌ Incomplete
            # ...
        )
```

**Problem**: Schema generator trusts `__gql_fields__` as the source of truth, but it's incomplete.

---

## 🎯 Root Cause Summary

The disconnect happens because:

1. **Decorator timing**: Fields added to `__annotations__` BEFORE `define_fraiseql_type()` runs
2. **Field collection logic**: `collect_fraise_fields()` only recognizes explicit `FraiseQLField` instances
3. **Schema generator assumption**: Assumes `__gql_fields__` is complete and authoritative

### Why This Happens

FraiseQL's architecture assumes:
- **All fields are explicitly defined** with `field()` or `fraise_field()`
- **Decorators don't modify field structure** (just add metadata)
- **`__gql_fields__` is the single source of truth**

But the auto-populate decorator violates these assumptions by:
- **Adding fields dynamically** after class definition
- **Using plain attributes** instead of `FraiseQLField` instances
- **Expecting schema to auto-discover** new fields

---

## 🔧 Why Rust Response Builder Works

**File**: `fraiseql_rs/src/mutation/response_builder.rs:100-112`

The Rust code works because it:
1. **Doesn't use Python metadata** - directly constructs JSON
2. **Hard-codes field names** - always adds id, message, status, errors
3. **Bypasses GraphQL validation** - adds fields to raw JSON response

This creates the mismatch: Rust adds fields that GraphQL schema doesn't know about.

---

## 📋 Key Data Structures

### What the decorator creates:

```python
# After @success decorator runs:
cls.__annotations__ = {
    "machine": Machine,
    "status": str,           # ✅ Added by decorator
    "message": str | None,   # ✅ Added by decorator
    "errors": list[Error] | None,  # ✅ Added by decorator
}

cls.status = "success"        # Plain attribute
cls.message = None            # Plain attribute
cls.errors = None             # Plain attribute
```

### What `define_fraiseql_type` creates:

```python
# After define_fraiseql_type() runs:
cls.__gql_fields__ = {
    "machine": FraiseQLField(name="machine", field_type=Machine, ...),
    # ❌ status, message, errors MISSING because they're plain attributes
}

cls.__gql_type_hints__ = {
    "machine": Machine,
    "status": str,           # ✅ Present
    "message": str | None,   # ✅ Present
    "errors": list[Error] | None,  # ✅ Present
}
```

### What schema generator sees:

```python
# In convert_type_to_graphql_output():
fields = getattr(typ, "__gql_fields__", {})
# Result: {"machine": FraiseQLField(...)}  ❌ Incomplete

type_hints = getattr(typ, "__gql_type_hints__", {})
# Result: {"machine": ..., "status": ..., "message": ..., "errors": ...}  ✅ Complete

# But schema generator loops over fields, not type_hints!
for name, field in fields.items():  # ❌ Only "machine"
    gql_fields[graphql_field_name] = GraphQLField(...)
```

---

## 🚩 Why Simple Fixes Won't Work

### ❌ Fix Attempt 1: Read from `__annotations__` in schema generator

**Problem**: Loses field metadata (descriptions, resolve_nested, sql_source, etc.)

```python
# Would need to reconstruct FraiseQLField from scratch
# But we don't know which fields should have special behavior
```

### ❌ Fix Attempt 2: Make decorator use `fraise_field()`

**Problem**: `fraise_field()` expects to be used at class definition time, not after

```python
# This doesn't work:
cls.status = fraise_field(default="success")  # Can't call after class created
```

### ❌ Fix Attempt 3: Modify `collect_fraise_fields` to auto-detect

**Problem**: Can't distinguish decorator-added fields from user-defined plain attributes

```python
# How do we know if a plain attribute should be a GraphQL field?
cls.status = "success"  # Decorator-added, should be in schema
cls._internal_cache = {}  # User-added, should NOT be in schema
```

---

## ✅ The Correct Fix

**Add decorator-injected fields to `__gql_fields__` explicitly** after `define_fraiseql_type()` returns.

This is correct because:
1. ✅ Decorator knows which fields it added
2. ✅ Can create proper `FraiseQLField` instances with metadata
3. ✅ Preserves existing field collection logic
4. ✅ No changes to schema generator needed
5. ✅ Backward compatible

See [Phase 2: Fix Implementation](./phase-2-fix-implementation.md) for detailed solution.

---

## 📝 Verification Questions

Before moving to Phase 2, confirm:

- [ ] Do you understand why `__gql_type_hints__` has the fields but `__gql_fields__` doesn't?
- [ ] Do you see why modifying the schema generator would lose field metadata?
- [ ] Do you agree that fixing the decorator is the right approach?
- [ ] Are there any edge cases we haven't considered?

**Next**: [Phase 2: Fix Implementation](./phase-2-fix-implementation.md)
