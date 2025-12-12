# Phase 2: Implement Dict Normalization [GREEN]

## Objective

Implement normalization logic to convert dict WHERE clauses to canonical `WhereClause` representation. This phase makes the Phase 1 dict normalization tests pass.

## Context

Phase 1 created the canonical `WhereClause` representation and defined tests. Now we implement the conversion logic for dict-based WHERE clauses.

Dict WHERE clauses come from:
- Direct API usage: `repo.find("view", where={"status": {"eq": "active"}})`
- GraphQL variables: `variables: {where: {machine: {id: {eq: "123"}}}}`
- Test code

The normalization must:
1. Detect FK vs JSONB lookups using metadata
2. Handle nested objects correctly
3. Support all operators (eq, in, contains, etc.)
4. Handle logical operators (AND, OR, NOT)

## Files to Create

- `src/fraiseql/where_normalization.py` - Normalization logic

## Files to Modify

- `src/fraiseql/db.py` - Add `_normalize_where()` method to `FraiseQLRepository`
- `tests/unit/test_where_normalization.py` - Un-skip dict normalization tests

## Implementation Steps

### Step 1: Create Normalization Module

Create `src/fraiseql/where_normalization.py`:

```python
"""WHERE clause normalization logic.

This module handles conversion of dict and WhereInput formats to the canonical
WhereClause representation.
"""

from __future__ import annotations

import logging
from typing import Any

from fraiseql.where_clause import FieldCondition, WhereClause
from fraiseql.db import _table_metadata

logger = logging.getLogger(__name__)


def normalize_dict_where(
    where_dict: dict[str, Any],
    view_name: str,
    table_columns: set[str] | None = None,
    jsonb_column: str = "data",
) -> WhereClause:
    """Normalize dict WHERE clause to canonical WhereClause.

    Args:
        where_dict: Dict-based WHERE clause
        view_name: Table/view name for metadata lookup
        table_columns: Set of actual table column names
        jsonb_column: JSONB column name (default: "data")

    Returns:
        Canonical WhereClause representation

    Examples:
        # Simple filter
        normalize_dict_where(
            {"status": {"eq": "active"}},
            "tv_allocation",
            {"status"}
        )
        # Returns: WhereClause with one FieldCondition using sql_column

        # Nested FK filter
        normalize_dict_where(
            {"machine": {"id": {"eq": "123"}}},
            "tv_allocation",
            {"machine_id", "data"}
        )
        # Returns: WhereClause with one FieldCondition using fk_column

        # Nested JSONB filter
        normalize_dict_where(
            {"device": {"name": {"eq": "Printer"}}},
            "tv_allocation",
            {"id", "data"}
        )
        # Returns: WhereClause with one FieldCondition using jsonb_path
    """
    # Get metadata if not provided
    if table_columns is None and view_name in _table_metadata:
        metadata = _table_metadata[view_name]
        if "columns" in metadata:
            table_columns = set(metadata["columns"])

    conditions = []
    nested_clauses = []
    not_clause = None
    logical_op = "AND"

    for field_name, field_value in where_dict.items():
        # Handle logical operators
        if field_name == "OR":
            # OR is a list of WHERE clauses
            if isinstance(field_value, list):
                or_conditions = []
                for or_dict in field_value:
                    or_clause = normalize_dict_where(
                        or_dict, view_name, table_columns, jsonb_column
                    )
                    or_conditions.extend(or_clause.conditions)

                # Create nested OR clause
                if or_conditions:
                    nested_clauses.append(
                        WhereClause(conditions=or_conditions, logical_op="OR")
                    )
            continue

        if field_name == "AND":
            # AND is a list of WHERE clauses
            if isinstance(field_value, list):
                for and_dict in field_value:
                    and_clause = normalize_dict_where(
                        and_dict, view_name, table_columns, jsonb_column
                    )
                    conditions.extend(and_clause.conditions)
            continue

        if field_name == "NOT":
            # NOT is a single WHERE clause
            if isinstance(field_value, dict):
                not_clause = normalize_dict_where(
                    field_value, view_name, table_columns, jsonb_column
                )
            continue

        # Regular field filter
        if not isinstance(field_value, dict):
            # Scalar value, wrap in eq operator
            field_value = {"eq": field_value}

        # Check if this is a nested object filter
        is_nested, use_fk = _is_nested_object_filter(
            field_name, field_value, table_columns, view_name
        )

        if is_nested and use_fk:
            # FK-based nested filter: machine.id → machine_id
            fk_column = f"{field_name}_id"

            # Extract nested filters
            for nested_field, nested_value in field_value.items():
                if nested_field == "id" and isinstance(nested_value, dict):
                    # This is the FK lookup
                    for op, val in nested_value.items():
                        if val is None:
                            continue

                        condition = FieldCondition(
                            field_path=[field_name, "id"],
                            operator=op,
                            value=val,
                            lookup_strategy="fk_column",
                            target_column=fk_column,
                        )
                        conditions.append(condition)

                        logger.debug(
                            f"Dict WHERE: FK nested filter {field_name}.id → {fk_column}",
                            extra={"condition": condition}
                        )
                else:
                    # Other nested fields use JSONB
                    if isinstance(nested_value, dict):
                        for op, val in nested_value.items():
                            if val is None:
                                continue

                            condition = FieldCondition(
                                field_path=[field_name, nested_field],
                                operator=op,
                                value=val,
                                lookup_strategy="jsonb_path",
                                target_column=jsonb_column,
                                jsonb_path=[field_name, nested_field],
                            )
                            conditions.append(condition)

        elif is_nested and not use_fk:
            # JSONB-based nested filter: device.name → data->'device'->>'name'
            for nested_field, nested_value in field_value.items():
                if isinstance(nested_value, dict):
                    for op, val in nested_value.items():
                        if val is None:
                            continue

                        condition = FieldCondition(
                            field_path=[field_name, nested_field],
                            operator=op,
                            value=val,
                            lookup_strategy="jsonb_path",
                            target_column=jsonb_column,
                            jsonb_path=[field_name, nested_field],
                        )
                        conditions.append(condition)

                        logger.debug(
                            f"Dict WHERE: JSONB nested filter {field_name}.{nested_field}",
                            extra={"condition": condition}
                        )

        else:
            # Direct column filter: status = 'active'
            # Check if this column exists in table_columns
            lookup_strategy = "sql_column"
            target_column = field_name

            if table_columns and field_name not in table_columns:
                # Column doesn't exist, might be in JSONB
                lookup_strategy = "jsonb_path"
                target_column = jsonb_column

            for op, val in field_value.items():
                if val is None:
                    continue

                if lookup_strategy == "jsonb_path":
                    condition = FieldCondition(
                        field_path=[field_name],
                        operator=op,
                        value=val,
                        lookup_strategy="jsonb_path",
                        target_column=jsonb_column,
                        jsonb_path=[field_name],
                    )
                else:
                    condition = FieldCondition(
                        field_path=[field_name],
                        operator=op,
                        value=val,
                        lookup_strategy="sql_column",
                        target_column=target_column,
                    )

                conditions.append(condition)

    return WhereClause(
        conditions=conditions,
        logical_op=logical_op,
        nested_clauses=nested_clauses,
        not_clause=not_clause,
    )


def _is_nested_object_filter(
    field_name: str,
    field_filter: dict,
    table_columns: set[str] | None,
    view_name: str,
) -> tuple[bool, bool]:
    """Detect if this is a nested object filter and how to handle it.

    Returns:
        Tuple of (is_nested, use_fk):
        - is_nested: True if this is a nested object filter
        - use_fk: True if should use FK column, False if should use JSONB path

    Logic:
        1. If field_filter has nested dict values → might be nested object
        2. Check if field_filter looks like {"id": {"eq": value}} → FK candidate
        3. Check if FK column exists in table_columns → use FK
        4. Otherwise → use JSONB path
    """
    # Check if metadata says this is a JSONB table
    is_jsonb_table = False
    if view_name in _table_metadata:
        is_jsonb_table = _table_metadata[view_name].get("has_jsonb_data", False)

    # Check if field_filter has nested operators
    # {"id": {"eq": value}} → nested
    # {"eq": value} → not nested
    has_nested_operator_values = any(
        isinstance(v, dict) and not any(k in ("OR", "AND", "NOT") for k in v)
        for v in field_filter.values()
    )

    if not has_nested_operator_values:
        return False, False

    # Check if this looks like a FK-based nested filter
    # Pattern: {"id": {"eq": value}}
    if "id" in field_filter and isinstance(field_filter["id"], dict):
        # Check if FK column exists
        potential_fk_column = f"{field_name}_id"

        if table_columns and potential_fk_column in table_columns:
            # FK column exists, use it
            logger.debug(
                f"Dict WHERE: Detected FK nested object filter for {field_name} "
                f"(FK column {potential_fk_column} exists)"
            )
            return True, True

    # Check if this is a JSONB-only nested filter
    # Any nested dict values + JSONB table → use JSONB path
    if has_nested_operator_values and is_jsonb_table:
        logger.debug(
            f"Dict WHERE: Detected JSONB nested filter for {field_name}"
        )
        return True, False

    return False, False
```

### Step 2: Add normalize_where() to FraiseQLRepository

Modify `src/fraiseql/db.py` to add normalization method:

```python
# Add import at top
from fraiseql.where_normalization import normalize_dict_where
from fraiseql.where_clause import WhereClause

class FraiseQLRepository:
    # ... existing code ...

    def _normalize_where(
        self,
        where: dict | Any,
        view_name: str,
        table_columns: set[str] | None = None,
    ) -> WhereClause:
        """Normalize WHERE clause to canonical WhereClause representation.

        This is the single entry point for WHERE normalization, handling both
        dict and WhereInput formats.

        Args:
            where: WHERE clause (dict or WhereInput object)
            view_name: Table/view name for metadata lookup
            table_columns: Set of actual table column names

        Returns:
            Canonical WhereClause representation

        Raises:
            TypeError: If where is not a supported type
        """
        # Already normalized
        if isinstance(where, WhereClause):
            return where

        # Dict-based WHERE
        if isinstance(where, dict):
            jsonb_column = "data"
            if view_name in _table_metadata:
                metadata = _table_metadata[view_name]
                if metadata.get("has_jsonb_data", False):
                    jsonb_column = metadata.get("jsonb_column", "data")

            return normalize_dict_where(
                where, view_name, table_columns, jsonb_column
            )

        # WhereInput-based WHERE (will implement in Phase 3)
        if hasattr(where, "_to_whereinput_dict"):
            # TODO: Implement in Phase 3
            raise NotImplementedError(
                "WhereInput normalization not yet implemented (Phase 3)"
            )

        raise TypeError(
            f"Unsupported WHERE type: {type(where)}. "
            f"Expected dict or WhereInput object."
        )
```

### Step 3: Enable Dict Normalization Tests

Modify `tests/unit/test_where_normalization.py`:

```python
# Un-skip dict normalization tests
class TestDictNormalization:
    """Test dict WHERE normalization."""

    # Remove @pytest.mark.skip from these tests:
    def test_normalize_simple_dict(self):
        """Test normalizing simple dict WHERE clause."""
        # ... test code ...

    def test_normalize_nested_fk_dict(self):
        """Test normalizing nested FK filter."""
        # ... test code ...

    def test_normalize_nested_jsonb_dict(self):
        """Test normalizing nested JSONB filter."""
        # ... test code ...

    # Keep WhereInput tests skipped (Phase 3)
```

### Step 4: Add Additional Dict Normalization Tests

Add more test cases to `tests/unit/test_where_normalization.py`:

```python
class TestDictNormalization:
    # ... existing tests ...

    def test_normalize_multiple_conditions(self):
        """Test normalizing multiple conditions."""
        where = {
            "status": {"eq": "active"},
            "machine": {"id": {"eq": uuid.UUID("12345678-1234-1234-1234-123456789abc")}}
        }

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"status", "machine_id", "data"}
        )

        assert len(clause.conditions) == 2
        # Should have both status and machine.id conditions

    def test_normalize_in_operator(self):
        """Test normalizing IN operator."""
        where = {"status": {"in": ["active", "pending"]}}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"status"}
        )

        assert len(clause.conditions) == 1
        assert clause.conditions[0].operator == "in"
        assert clause.conditions[0].value == ["active", "pending"]

    def test_normalize_or_clause(self):
        """Test normalizing OR logical operator."""
        where = {
            "OR": [
                {"status": {"eq": "active"}},
                {"status": {"eq": "pending"}}
            ]
        }

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"status"}
        )

        # Should have nested OR clause
        assert len(clause.nested_clauses) == 1
        assert clause.nested_clauses[0].logical_op == "OR"
        assert len(clause.nested_clauses[0].conditions) == 2

    def test_normalize_not_clause(self):
        """Test normalizing NOT logical operator."""
        where = {
            "status": {"eq": "active"},
            "NOT": {"machine_id": {"isnull": True}}
        }

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"status", "machine_id"}
        )

        assert len(clause.conditions) == 1
        assert clause.not_clause is not None
        assert len(clause.not_clause.conditions) == 1

    def test_normalize_mixed_fk_and_jsonb(self):
        """Test normalizing mixed FK and JSONB filters on same object."""
        where = {
            "machine": {
                "id": {"eq": uuid.UUID("12345678-1234-1234-1234-123456789abc")},
                "name": {"contains": "Printer"}
            }
        }

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"machine_id", "data"}
        )

        assert len(clause.conditions) == 2
        # First condition should use FK
        fk_cond = [c for c in clause.conditions if c.field_path == ["machine", "id"]][0]
        assert fk_cond.lookup_strategy == "fk_column"
        assert fk_cond.target_column == "machine_id"

        # Second condition should use JSONB
        jsonb_cond = [c for c in clause.conditions if c.field_path == ["machine", "name"]][0]
        assert jsonb_cond.lookup_strategy == "jsonb_path"
        assert jsonb_cond.jsonb_path == ["machine", "name"]

    def test_normalize_contains_operator(self):
        """Test normalizing string contains operator."""
        where = {"name": {"contains": "test"}}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"name"}
        )

        assert len(clause.conditions) == 1
        assert clause.conditions[0].operator == "contains"

    def test_normalize_isnull_operator(self):
        """Test normalizing IS NULL operator."""
        where = {"machine_id": {"isnull": True}}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"machine_id"}
        )

        assert len(clause.conditions) == 1
        assert clause.conditions[0].operator == "isnull"
        assert clause.conditions[0].value is True

    def test_normalize_scalar_value_wraps_in_eq(self):
        """Test scalar value gets wrapped in eq operator."""
        where = {"status": "active"}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"status"}
        )

        assert len(clause.conditions) == 1
        assert clause.conditions[0].operator == "eq"
        assert clause.conditions[0].value == "active"

    def test_normalize_nonexistent_column_uses_jsonb(self):
        """Test filtering on column not in table_columns uses JSONB."""
        where = {"custom_field": {"eq": "value"}}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"id", "status", "data"}  # custom_field not in columns
        )

        assert len(clause.conditions) == 1
        assert clause.conditions[0].lookup_strategy == "jsonb_path"
        assert clause.conditions[0].target_column == "data"
        assert clause.conditions[0].jsonb_path == ["custom_field"]
```

## Verification Commands

```bash
# Run dict normalization tests (should pass now)
uv run pytest tests/unit/test_where_normalization.py::TestDictNormalization -v

# Run all WHERE tests
uv run pytest tests/unit/test_where_clause.py tests/unit/test_where_normalization.py -v

# Check code coverage
uv run pytest tests/unit/test_where_normalization.py --cov=fraiseql.where_normalization --cov-report=term-missing

# Verify logging output
uv run pytest tests/unit/test_where_normalization.py::TestDictNormalization::test_normalize_nested_fk_dict -v -s

# Run full test suite to ensure no regressions
uv run pytest tests/ -v
```

## Acceptance Criteria

- [ ] `where_normalization.py` created with `normalize_dict_where()` function
- [ ] `_is_nested_object_filter()` correctly detects FK vs JSONB lookups
- [ ] `_normalize_where()` method added to `FraiseQLRepository`
- [ ] All dict normalization tests pass (15+ tests)
- [ ] FK nested filters correctly resolve to FK columns
- [ ] JSONB nested filters correctly resolve to JSONB paths
- [ ] Mixed FK+JSONB filters work correctly
- [ ] Logical operators (AND, OR, NOT) handled correctly
- [ ] All operators supported (eq, in, contains, isnull, etc.)
- [ ] Structured logging shows FK vs JSONB detection
- [ ] Code coverage >85% for normalization logic
- [ ] No regressions in existing tests

## DO NOT

- ❌ Modify SQL generation code yet (that's Phase 4)
- ❌ Remove existing WHERE processing code (keep as fallback)
- ❌ Implement WhereInput normalization (that's Phase 3)
- ❌ Change `_build_where_clause()` to use normalization yet

## Notes

This is a **GREEN phase**: We implement logic to make the RED phase tests pass.

The normalization logic is critical - it must correctly distinguish between:
1. FK-based nested filters (use FK column for performance)
2. JSONB-based nested filters (use JSONB path)
3. Direct column filters (use SQL column)

The FK detection logic from the current `_is_nested_object_filter()` in `db.py` should be preserved and enhanced.

## Next Phase

**Phase 3:** Implement WhereInput normalization to convert WhereInput objects to `WhereClause`.
