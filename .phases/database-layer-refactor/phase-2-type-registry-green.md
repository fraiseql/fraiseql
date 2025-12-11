# Phase 2: Type Registry & Metadata

**Phase:** GREEN (Make Tests Pass)
**Duration:** 4-6 hours
**Risk:** Low

---

## Objective

**TDD Phase GREEN:** Implement type registration system to make Phase 1 tests pass.

Extract from `db.py`:
- `_type_registry` global
- `_table_metadata` global
- `register_type_for_view()` function
- Type lookup methods from `FraiseQLRepository`

This is the **LEAST COUPLED** component - perfect starting point.

---

## Files to Create

### 1. `src/fraiseql/db/registry/type_registry.py`

Extract type registration logic from lines 26-187 of db.py:

```python
"""Type registration system."""

import logging
from typing import Any, Optional

logger = logging.getLogger(__name__)


class TypeRegistry:
    """
    Registry for GraphQL types mapped to database views.

    Stores type classes and metadata for development mode type instantiation.
    """

    def __init__(self):
        self._types: dict[str, type] = {}
        self._metadata: dict[str, dict[str, Any]] = {}

    def register_type(
        self,
        view_name: str,
        type_class: type,
        table_columns: set[str] | None = None,
        has_jsonb_data: bool | None = None,
        jsonb_column: str | None = None,
        fk_relationships: dict[str, str] | None = None,
        validate_fk_strict: bool = True,
    ) -> None:
        """Register a type class for a specific view name with optional metadata.

        [... copy docstring from db.py:register_type_for_view() ...]
        """
        # Copy implementation from db.py lines 150-186
        self._types[view_name] = type_class
        logger.debug(f"Registered type {type_class.__name__} for view {view_name}")

        # Initialize FK relationships
        fk_relationships = fk_relationships or {}

        # Validate FK relationships if strict mode
        if validate_fk_strict and fk_relationships and table_columns:
            for field_name, fk_column in fk_relationships.items():
                if fk_column not in table_columns:
                    raise ValueError(
                        f"Invalid FK relationship for {view_name}: "
                        f"Field '{field_name}' mapped to FK column '{fk_column}', "
                        f"but '{fk_column}' not in table_columns: {table_columns}. "
                        f"Either add '{fk_column}' to table_columns or fix fk_relationships. "
                        f"To allow this (not recommended), set validate_fk_strict=False."
                    )

        # Store metadata if provided
        if (
            table_columns is not None
            or has_jsonb_data is not None
            or jsonb_column is not None
            or fk_relationships
        ):
            metadata = {
                "columns": table_columns or set(),
                "has_jsonb_data": has_jsonb_data or False,
                "jsonb_column": jsonb_column,
                "fk_relationships": fk_relationships,
                "validate_fk_strict": validate_fk_strict,
            }
            self._metadata[view_name] = metadata
            logger.debug(
                f"Registered metadata for {view_name}: {len(table_columns or set())} columns, "
                f"jsonb={has_jsonb_data}, jsonb_column={jsonb_column}"
            )

    def get_type(self, view_name: str) -> Optional[type]:
        """Get registered type for a view."""
        return self._types.get(view_name)

    def get_metadata(self, view_name: str) -> Optional[dict[str, Any]]:
        """Get registered metadata for a view."""
        return self._metadata.get(view_name)

    def has_type(self, view_name: str) -> bool:
        """Check if a type is registered."""
        return view_name in self._types


# Global registry instance
_default_registry = TypeRegistry()


def register_type_for_view(
    view_name: str,
    type_class: type,
    table_columns: set[str] | None = None,
    has_jsonb_data: bool | None = None,
    jsonb_column: str | None = None,
    fk_relationships: dict[str, str] | None = None,
    validate_fk_strict: bool = True,
) -> None:
    """Register a type class with the default registry.

    Backward-compatible function that uses the global registry.
    """
    _default_registry.register_type(
        view_name=view_name,
        type_class=type_class,
        table_columns=table_columns,
        has_jsonb_data=has_jsonb_data,
        jsonb_column=jsonb_column,
        fk_relationships=fk_relationships,
        validate_fk_strict=validate_fk_strict,
    )


def get_default_registry() -> TypeRegistry:
    """Get the default type registry."""
    return _default_registry
```

### 2. `src/fraiseql/db/registry/__init__.py`

```python
"""Type registry for database views."""

from .type_registry import (
    TypeRegistry,
    register_type_for_view,
    get_default_registry,
)

__all__ = [
    "TypeRegistry",
    "register_type_for_view",
    "get_default_registry",
]
```

---

## Files to Modify

### 1. Update `src/fraiseql/db.py` (TEMPORARY SHIM)

Add at top of file (after imports):

```python
# TEMPORARY: Import from new registry module while maintaining backward compatibility
from fraiseql.db.registry import register_type_for_view, get_default_registry

# DEPRECATED: These globals are maintained for backward compatibility
# They reference the new registry implementation
_type_registry = get_default_registry()._types
_table_metadata = get_default_registry()._metadata
```

This allows old code to keep working while we migrate.

---

## Implementation Steps

### Step 1: Create TypeRegistry Class (2 hours)
1. Copy type registration logic from db.py
2. Create TypeRegistry class
3. Implement register_type(), get_type(), get_metadata()
4. Create global registry instance

### Step 2: Add Backward Compatibility (1 hour)
1. Create register_type_for_view() function
2. Update db.py to import from new module
3. Keep _type_registry and _table_metadata as references

### Step 3: Update Tests (1 hour)
1. Ensure Phase 1 tests pass
2. Add additional test cases
3. Test backward compatibility

### Step 4: Integration Testing (1 hour)
1. Run full test suite
2. Verify no regressions
3. Check that old code still works

---

## Verification Commands

```bash
# Run type registry tests (should now PASS)
uv run pytest tests/unit/db/registry/ -v

# Run full test suite (should still pass)
uv run pytest tests/unit/ -v
uv run pytest tests/integration/ -v

# Verify backward compatibility
python -c "from fraiseql.db import register_type_for_view; print('OK')"

# Verify new module works
python -c "from fraiseql.db.registry import TypeRegistry; print('OK')"
```

---

## Acceptance Criteria

- [ ] TypeRegistry class implemented
- [ ] All Phase 1 type registry tests PASS
- [ ] register_type_for_view() works from fraiseql.db (backward compat)
- [ ] register_type_for_view() works from fraiseql.db.registry (new)
- [ ] All 4,943+ tests still passing
- [ ] No performance regression
- [ ] Old code unchanged (still imports from db.py)

---

## DO NOT

- ❌ Delete or modify original code in db.py beyond the shim
- ❌ Change any behavior of type registration
- ❌ Break backward compatibility
- ❌ Skip integration testing

---

## Next Phase

Once type registry is extracted and tests pass:
→ **Phase 3:** Query Builder (extract query building logic)
