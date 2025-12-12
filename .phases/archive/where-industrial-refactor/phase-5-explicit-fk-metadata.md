# Phase 5: Add Explicit FK Metadata [GREEN]

## Objective

Make FK relationships explicit in metadata, eliminating runtime FK detection guesswork.

## Context

Currently, FK detection uses convention: `machine` field → `machine_id` column.

This is implicit and can break if:
- Column is named differently (`printer_id` for `machine` field)
- Field exists in JSONB but shouldn't be treated as FK
- Ambiguous cases

Making FK relationships explicit improves:
- Validation at generation time
- Better error messages
- No runtime guesswork
- Easier to debug

## Files to Modify

- `src/fraiseql/db.py` - Add `fk_relationships` param to `register_type_for_view()`
- `src/fraiseql/sql/graphql_where_generator.py` - Attach FK metadata to WhereInput
- `src/fraiseql/where_normalization.py` - Use explicit FK metadata when available
- Tests - Update `register_type_for_view()` calls

## Implementation Steps

### Step 1: Add fk_relationships Parameter with Strict Validation

**IMPORTANT: Strict validation by default to catch errors early**

```python
def register_type_for_view(
    view_name: str,
    type_cls: type,
    *,
    table_columns: set[str] | None = None,
    has_jsonb_data: bool = False,
    jsonb_column: str = "data",
    fk_relationships: dict[str, str] | None = None,  # NEW
    validate_fk_strict: bool = True,  # NEW - strict by default
):
    """Register metadata for a view/table type.

    Args:
        fk_relationships: Map GraphQL field name → FK column name.
            Example: {"machine": "machine_id", "printer": "printer_id"}
            If not specified, uses convention: field + "_id"
        validate_fk_strict: If True, raise error on FK validation failures.
            If False, only warn (useful for legacy code migration).

    Raises:
        ValueError: If validate_fk_strict=True and FK relationships are invalid
    """
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

    _table_metadata[view_name] = {
        "type_class": type_cls,
        "columns": table_columns,
        "has_jsonb_data": has_jsonb_data,
        "jsonb_column": jsonb_column,
        "fk_relationships": fk_relationships,
        "validate_fk_strict": validate_fk_strict,
    }
```

### Step 2: Use Explicit FK Metadata in Normalization with Strict Mode

```python
def _is_nested_object_filter(
    field_name: str,
    field_filter: dict,
    table_columns: set[str] | None,
    view_name: str,
) -> tuple[bool, bool]:
    """Detect nested object filter with explicit FK metadata support."""

    # Check for explicit FK metadata first
    if view_name in _table_metadata:
        metadata = _table_metadata[view_name]
        fk_relationships = metadata.get("fk_relationships", {})
        validate_strict = metadata.get("validate_fk_strict", True)

        # Explicit FK declared?
        if field_name in fk_relationships:
            fk_column = fk_relationships[field_name]

            # Verify FK column exists
            if table_columns and fk_column in table_columns:
                logger.debug(
                    f"Using explicit FK relationship: {field_name} → {fk_column}"
                )
                return True, True
            else:
                error_msg = (
                    f"FK relationship declared ({field_name} → {fk_column}) "
                    f"but column '{fk_column}' not in table_columns. "
                )

                if validate_strict:
                    # Strict mode: this should have been caught at registration
                    # If we get here, it's a bug
                    raise RuntimeError(
                        error_msg + "This should have been caught during registration."
                    )
                else:
                    # Lenient mode: warn and fallback to JSONB
                    logger.warning(error_msg + "Using JSONB fallback.")

    # Fallback to convention-based detection
    # ... existing logic ...
```

### Step 3: Attach FK Metadata to WhereInput Classes

```python
def create_graphql_where_input(cls: type, name: str | None = None) -> type:
    """Generate WhereInput with FK metadata."""

    # ... existing code ...

    # Get FK relationships from metadata
    sql_source = get_sql_source(cls)
    fk_relationships = {}

    if sql_source and sql_source in _table_metadata:
        fk_relationships = _table_metadata[sql_source].get("fk_relationships", {})

    # Attach metadata to class
    WhereInputClass.__table_name__ = sql_source
    WhereInputClass.__fk_relationships__ = fk_relationships

    # Generate documentation
    if fk_relationships:
        fk_doc = "\\n".join(
            f"    - {field} → FK column '{col}'"
            for field, col in fk_relationships.items()
        )
        WhereInputClass.__doc__ = f"""{WhereInputClass.__doc__ or ''}

FK Relationships:
{fk_doc}
"""

    return WhereInputClass
```

### Step 4: Validation at Generation Time

```python
def create_graphql_where_input(cls: type, name: str | None = None) -> type:
    """Generate WhereInput with validation."""

    # ... after field generation ...

    # Validate FK relationships
    sql_source = get_sql_source(cls)
    if sql_source and sql_source in _table_metadata:
        metadata = _table_metadata[sql_source]
        fk_relationships = metadata.get("fk_relationships", {})
        table_columns = metadata.get("columns", set())

        # Validate declared FKs exist
        for field_name, fk_column in fk_relationships.items():
            if table_columns and fk_column not in table_columns:
                logger.warning(
                    f"FK relationship {field_name} → {fk_column} declared "
                    f"but {fk_column} not in registered columns for {sql_source}"
                )

        # Check for undeclared FK candidates
        type_hints = get_type_hints(cls)
        for field_name, field_type in type_hints.items():
            if _is_fraise_type(field_type):
                potential_fk = f"{field_name}_id"
                if (
                    field_name not in fk_relationships
                    and table_columns
                    and potential_fk in table_columns
                ):
                    logger.info(
                        f"Field {cls.__name__}.{field_name} looks like FK relationship "
                        f"(column {potential_fk} exists) but not declared in fk_relationships. "
                        f"Using convention-based detection."
                    )

    return WhereInputClass
```

## Verification Commands

```bash
# Update tests with explicit FK metadata
uv run pytest tests/regression/test_nested_filter_id_field.py -v

# Verify validation warnings
uv run pytest tests/unit/test_type_registration.py -v -s  # if exists

# Run full suite
uv run pytest tests/ -v
```

## Acceptance Criteria

- [ ] `fk_relationships` parameter added to `register_type_for_view()`
- [ ] **`validate_fk_strict` parameter added (defaults to True)**
- [ ] **Strict validation raises errors on FK mismatches**
- [ ] Explicit FK metadata used when available
- [ ] Convention-based detection still works as fallback
- [ ] FK metadata attached to WhereInput classes
- [ ] Validation raises errors in strict mode (default)
- [ ] Lenient mode available for legacy code (validate_fk_strict=False)
- [ ] Documentation generated with FK info
- [ ] All tests pass
- [ ] Test coverage includes strict/lenient validation modes

## Notes

This makes FK relationships explicit and self-documenting. Users can see which fields use FK optimization.

## Next Phase

**Phase 6:** Remove old code paths and clean up.
