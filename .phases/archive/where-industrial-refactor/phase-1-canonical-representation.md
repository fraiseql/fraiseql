# Phase 1: Define Canonical Representation [RED]

## Objective

Create type-safe canonical representation for WHERE clauses with comprehensive test coverage. This phase follows TDD: write tests first, expect failures, then implement in Phase 2.

## Context

Currently, FraiseQL has no canonical internal representation for WHERE clauses:
- Dicts are flexible but untyped
- WhereInput objects convert to SQL immediately
- SQL objects are binary and hard to inspect

We need a typed, inspectable intermediate form that serves as the single source of truth.

## Files to Create

- `src/fraiseql/where_clause.py` - Canonical representation dataclasses
- `tests/unit/test_where_clause.py` - Unit tests for WhereClause
- `tests/unit/test_where_normalization.py` - Tests for normalization logic (will fail initially)

## Files to Modify

None (this phase only adds new code, no modifications)

## Implementation Steps

### Step 1: Define FieldCondition Dataclass

Create `src/fraiseql/where_clause.py`:

```python
"""Canonical representation for WHERE clauses.

This module defines the internal representation used by FraiseQL for all WHERE
clause processing, regardless of input format (dict or WhereInput).

Architecture:
    User Input (dict/WhereInput)
        → Normalize to WhereClause
        → Generate SQL
        → PostgreSQL
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal
from psycopg.sql import SQL, Composed, Identifier, Literal as SQLLiteral


# Supported operators
COMPARISON_OPERATORS = {
    "eq": "=",
    "neq": "!=",
    "gt": ">",
    "gte": ">=",
    "lt": "<",
    "lte": "<=",
}

CONTAINMENT_OPERATORS = {
    "in": "IN",
    "nin": "NOT IN",
}

STRING_OPERATORS = {
    "contains": "LIKE",
    "icontains": "ILIKE",
    "startswith": "LIKE",
    "istartswith": "ILIKE",
    "endswith": "LIKE",
    "iendswith": "ILIKE",
}

NULL_OPERATORS = {
    "isnull": "IS NULL",
}

ALL_OPERATORS = {
    **COMPARISON_OPERATORS,
    **CONTAINMENT_OPERATORS,
    **STRING_OPERATORS,
    **NULL_OPERATORS,
}


@dataclass
class FieldCondition:
    """Single filter condition on a field.

    Represents a single comparison like: machine_id = '123' or data->'device'->>'name' = 'Printer'

    Attributes:
        field_path: Path to the field, e.g., ["machine", "id"] for nested filter
        operator: Filter operator like "eq", "neq", "in", "contains"
        value: The value to compare against
        lookup_strategy: How to access this field in SQL
            - "fk_column": Use FK column (e.g., machine_id)
            - "jsonb_path": Use JSONB path (e.g., data->'machine'->>'id')
            - "sql_column": Use direct column (e.g., status)
        target_column: The actual SQL column name
            - For FK: "machine_id"
            - For JSONB: "data" (with jsonb_path set)
            - For SQL: "status"
        jsonb_path: For JSONB lookups, the path within the data column
            - e.g., ["machine", "id"] → data->'machine'->>'id'

    Examples:
        # FK lookup: machine.id = '123'
        FieldCondition(
            field_path=["machine", "id"],
            operator="eq",
            value=UUID("123"),
            lookup_strategy="fk_column",
            target_column="machine_id",
            jsonb_path=None
        )

        # JSONB lookup: device.name = 'Printer'
        FieldCondition(
            field_path=["device", "name"],
            operator="eq",
            value="Printer",
            lookup_strategy="jsonb_path",
            target_column="data",
            jsonb_path=["device", "name"]
        )

        # Direct column: status = 'active'
        FieldCondition(
            field_path=["status"],
            operator="eq",
            value="active",
            lookup_strategy="sql_column",
            target_column="status",
            jsonb_path=None
        )
    """

    field_path: list[str]
    operator: str
    value: Any
    lookup_strategy: Literal["fk_column", "jsonb_path", "sql_column"]
    target_column: str
    jsonb_path: list[str] | None = None

    def __post_init__(self):
        """Validate the condition after initialization."""
        # Validate operator
        if self.operator not in ALL_OPERATORS:
            raise ValueError(
                f"Invalid operator '{self.operator}'. "
                f"Supported operators: {', '.join(sorted(ALL_OPERATORS.keys()))}"
            )

        # Validate lookup_strategy
        valid_strategies = {"fk_column", "jsonb_path", "sql_column"}
        if self.lookup_strategy not in valid_strategies:
            raise ValueError(
                f"Invalid lookup_strategy '{self.lookup_strategy}'. "
                f"Must be one of: {', '.join(sorted(valid_strategies))}"
            )

        # Validate JSONB path consistency
        if self.lookup_strategy == "jsonb_path" and not self.jsonb_path:
            raise ValueError(
                "lookup_strategy='jsonb_path' requires jsonb_path to be set"
            )

        # Validate field_path
        if not self.field_path:
            raise ValueError("field_path cannot be empty")

    def to_sql(self) -> tuple[Composed, list[Any]]:
        """Generate SQL for this condition.

        Returns:
            Tuple of (SQL Composed object, list of parameters)

        Examples:
            # FK column: machine_id = %s
            SQL: Identifier("machine_id") + SQL(" = ") + SQL("%s")
            Params: [UUID("123")]

            # JSONB path: data->'device'->>'name' = %s
            SQL: SQL("data->'device'->>'name' = %s")
            Params: ["Printer"]
        """
        params = []

        if self.lookup_strategy == "fk_column":
            # FK column lookup: machine_id = %s
            sql_op = ALL_OPERATORS[self.operator]

            if self.operator in CONTAINMENT_OPERATORS:
                # IN/NOT IN: machine_id IN %s
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(tuple(self.value) if isinstance(self.value, list) else self.value)
            elif self.operator == "isnull":
                # IS NULL / IS NOT NULL
                null_op = "IS NULL" if self.value else "IS NOT NULL"
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {null_op}")
                ])
            else:
                # Standard comparison: machine_id = %s
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(self.value)

        elif self.lookup_strategy == "jsonb_path":
            # JSONB path lookup: data->'device'->>'name' = %s
            sql_op = ALL_OPERATORS[self.operator]

            # Build JSONB path: data->'device'->>'name'
            if not self.jsonb_path:
                raise ValueError("jsonb_path required for jsonb_path lookup")

            # Start with the data column
            path_parts = [Identifier(self.target_column)]

            # Add intermediate keys with ->
            for i, key in enumerate(self.jsonb_path[:-1]):
                path_parts.append(SQL("->"))
                path_parts.append(SQLLiteral(key))

            # Add final key with ->> (text extraction)
            path_parts.append(SQL("->>"))
            path_parts.append(SQLLiteral(self.jsonb_path[-1]))

            jsonb_expr = Composed(path_parts)

            if self.operator in CONTAINMENT_OPERATORS:
                sql = Composed([
                    jsonb_expr,
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(tuple(self.value) if isinstance(self.value, list) else self.value)
            elif self.operator == "isnull":
                null_op = "IS NULL" if self.value else "IS NOT NULL"
                sql = Composed([jsonb_expr, SQL(f" {null_op}")])
            elif self.operator in STRING_OPERATORS:
                # LIKE/ILIKE with pattern
                pattern = self._build_like_pattern()
                sql = Composed([
                    jsonb_expr,
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(pattern)
            else:
                sql = Composed([
                    jsonb_expr,
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(str(self.value))  # JSONB text comparison

        elif self.lookup_strategy == "sql_column":
            # Direct SQL column: status = %s
            sql_op = ALL_OPERATORS[self.operator]

            if self.operator in CONTAINMENT_OPERATORS:
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(tuple(self.value) if isinstance(self.value, list) else self.value)
            elif self.operator == "isnull":
                null_op = "IS NULL" if self.value else "IS NOT NULL"
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {null_op}")
                ])
            elif self.operator in STRING_OPERATORS:
                pattern = self._build_like_pattern()
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(pattern)
            else:
                sql = Composed([
                    Identifier(self.target_column),
                    SQL(f" {sql_op} "),
                    SQL("%s")
                ])
                params.append(self.value)

        else:
            raise ValueError(f"Unknown lookup_strategy: {self.lookup_strategy}")

        return sql, params

    def _build_like_pattern(self) -> str:
        """Build LIKE pattern from operator and value."""
        if self.operator in ("contains", "icontains"):
            return f"%{self.value}%"
        elif self.operator in ("startswith", "istartswith"):
            return f"{self.value}%"
        elif self.operator in ("endswith", "iendswith"):
            return f"%{self.value}"
        else:
            return str(self.value)

    def __repr__(self) -> str:
        """Human-readable representation for debugging."""
        path_str = ".".join(self.field_path)

        if self.lookup_strategy == "fk_column":
            target = f"FK:{self.target_column}"
        elif self.lookup_strategy == "jsonb_path":
            jsonb_path_str = ".".join(self.jsonb_path or [])
            target = f"JSONB:{self.target_column}[{jsonb_path_str}]"
        else:
            target = f"COL:{self.target_column}"

        return f"FieldCondition({path_str} {self.operator} {self.value!r} → {target})"


@dataclass
class WhereClause:
    """Canonical representation of a WHERE clause.

    Represents the complete WHERE clause including multiple conditions,
    logical operators (AND/OR/NOT), and nested sub-clauses.

    Attributes:
        conditions: List of field conditions (combined with logical_op)
        logical_op: How to combine conditions ("AND" or "OR")
        nested_clauses: Sub-clauses for complex queries
        not_clause: Optional NOT clause

    Examples:
        # Simple: status = 'active'
        WhereClause(
            conditions=[
                FieldCondition(field_path=["status"], operator="eq", value="active", ...)
            ]
        )

        # Multiple conditions: status = 'active' AND machine_id = '123'
        WhereClause(
            conditions=[
                FieldCondition(field_path=["status"], ...),
                FieldCondition(field_path=["machine", "id"], ...)
            ],
            logical_op="AND"
        )

        # Nested: (status = 'active' OR status = 'pending') AND machine_id = '123'
        WhereClause(
            conditions=[
                FieldCondition(field_path=["machine", "id"], ...)
            ],
            nested_clauses=[
                WhereClause(
                    conditions=[
                        FieldCondition(field_path=["status"], operator="eq", value="active", ...),
                        FieldCondition(field_path=["status"], operator="eq", value="pending", ...)
                    ],
                    logical_op="OR"
                )
            ]
        )
    """

    conditions: list[FieldCondition] = field(default_factory=list)
    logical_op: Literal["AND", "OR"] = "AND"
    nested_clauses: list[WhereClause] = field(default_factory=list)
    not_clause: WhereClause | None = None

    def __post_init__(self):
        """Validate the WHERE clause."""
        if self.logical_op not in ("AND", "OR"):
            raise ValueError(f"Invalid logical_op '{self.logical_op}'. Must be 'AND' or 'OR'")

        # Must have at least one condition or nested clause
        if not self.conditions and not self.nested_clauses and not self.not_clause:
            raise ValueError("WhereClause must have at least one condition, nested clause, or NOT clause")

    def to_sql(self) -> tuple[Composed | None, list[Any]]:
        """Generate SQL for this WHERE clause.

        Returns:
            Tuple of (SQL Composed object or None, list of parameters)

        Examples:
            # Simple: status = %s
            SQL: Identifier("status") + SQL(" = ") + SQL("%s")
            Params: ["active"]

            # Multiple: status = %s AND machine_id = %s
            SQL: Identifier("status") + ... + SQL(" AND ") + Identifier("machine_id") + ...
            Params: ["active", UUID("123")]
        """
        all_parts = []
        all_params = []

        # Generate SQL for each condition
        for condition in self.conditions:
            sql, params = condition.to_sql()
            all_parts.append(sql)
            all_params.extend(params)

        # Generate SQL for nested clauses
        for nested in self.nested_clauses:
            nested_sql, nested_params = nested.to_sql()
            if nested_sql:
                # Wrap in parentheses
                wrapped = Composed([SQL("("), nested_sql, SQL(")")])
                all_parts.append(wrapped)
                all_params.extend(nested_params)

        # Generate SQL for NOT clause
        if self.not_clause:
            not_sql, not_params = self.not_clause.to_sql()
            if not_sql:
                wrapped = Composed([SQL("NOT ("), not_sql, SQL(")")])
                all_parts.append(wrapped)
                all_params.extend(not_params)

        # Combine with logical operator
        if not all_parts:
            return None, []

        if len(all_parts) == 1:
            return all_parts[0], all_params

        # Join with AND/OR
        separator = SQL(f" {self.logical_op} ")
        combined_sql = separator.join(all_parts)

        return combined_sql, all_params

    def __repr__(self) -> str:
        """Human-readable representation for debugging."""
        parts = []

        if self.conditions:
            cond_strs = [str(c) for c in self.conditions]
            parts.append(f" {self.logical_op} ".join(cond_strs))

        if self.nested_clauses:
            for nested in self.nested_clauses:
                parts.append(f"({nested!r})")

        if self.not_clause:
            parts.append(f"NOT ({self.not_clause!r})")

        return f"WhereClause({' AND '.join(parts)})"
```

### Step 2: Create Unit Tests for WhereClause

Create `tests/unit/test_where_clause.py`:

```python
"""Unit tests for WhereClause canonical representation.

These tests verify the dataclass validation, SQL generation, and edge cases
for the canonical WHERE representation.
"""

import uuid
from datetime import date

import pytest
from psycopg.sql import SQL, Composed

from fraiseql.where_clause import (
    ALL_OPERATORS,
    FieldCondition,
    WhereClause,
)


class TestFieldCondition:
    """Test FieldCondition dataclass."""

    def test_create_fk_condition(self):
        """Test creating FK column condition."""
        condition = FieldCondition(
            field_path=["machine", "id"],
            operator="eq",
            value=uuid.UUID("12345678-1234-1234-1234-123456789abc"),
            lookup_strategy="fk_column",
            target_column="machine_id",
        )

        assert condition.field_path == ["machine", "id"]
        assert condition.operator == "eq"
        assert condition.lookup_strategy == "fk_column"
        assert condition.target_column == "machine_id"
        assert condition.jsonb_path is None

    def test_create_jsonb_condition(self):
        """Test creating JSONB path condition."""
        condition = FieldCondition(
            field_path=["device", "name"],
            operator="eq",
            value="Printer",
            lookup_strategy="jsonb_path",
            target_column="data",
            jsonb_path=["device", "name"],
        )

        assert condition.field_path == ["device", "name"]
        assert condition.lookup_strategy == "jsonb_path"
        assert condition.jsonb_path == ["device", "name"]

    def test_create_sql_column_condition(self):
        """Test creating direct SQL column condition."""
        condition = FieldCondition(
            field_path=["status"],
            operator="eq",
            value="active",
            lookup_strategy="sql_column",
            target_column="status",
        )

        assert condition.field_path == ["status"]
        assert condition.lookup_strategy == "sql_column"
        assert condition.target_column == "status"

    def test_invalid_operator_raises_error(self):
        """Test invalid operator raises ValueError."""
        with pytest.raises(ValueError, match="Invalid operator 'invalid'"):
            FieldCondition(
                field_path=["status"],
                operator="invalid",
                value="active",
                lookup_strategy="sql_column",
                target_column="status",
            )

    def test_invalid_lookup_strategy_raises_error(self):
        """Test invalid lookup_strategy raises ValueError."""
        with pytest.raises(ValueError, match="Invalid lookup_strategy"):
            FieldCondition(
                field_path=["status"],
                operator="eq",
                value="active",
                lookup_strategy="invalid",
                target_column="status",
            )

    def test_jsonb_without_path_raises_error(self):
        """Test JSONB lookup without jsonb_path raises ValueError."""
        with pytest.raises(ValueError, match="requires jsonb_path"):
            FieldCondition(
                field_path=["device", "name"],
                operator="eq",
                value="Printer",
                lookup_strategy="jsonb_path",
                target_column="data",
                jsonb_path=None,  # Missing!
            )

    def test_empty_field_path_raises_error(self):
        """Test empty field_path raises ValueError."""
        with pytest.raises(ValueError, match="field_path cannot be empty"):
            FieldCondition(
                field_path=[],
                operator="eq",
                value="active",
                lookup_strategy="sql_column",
                target_column="status",
            )

    def test_fk_condition_to_sql(self):
        """Test FK condition generates correct SQL."""
        condition = FieldCondition(
            field_path=["machine", "id"],
            operator="eq",
            value=uuid.UUID("12345678-1234-1234-1234-123456789abc"),
            lookup_strategy="fk_column",
            target_column="machine_id",
        )

        sql, params = condition.to_sql()

        assert isinstance(sql, Composed)
        assert "machine_id" in sql.as_string(None)
        assert "=" in sql.as_string(None)
        assert len(params) == 1
        assert params[0] == uuid.UUID("12345678-1234-1234-1234-123456789abc")

    def test_jsonb_condition_to_sql(self):
        """Test JSONB condition generates correct SQL."""
        condition = FieldCondition(
            field_path=["device", "name"],
            operator="eq",
            value="Printer",
            lookup_strategy="jsonb_path",
            target_column="data",
            jsonb_path=["device", "name"],
        )

        sql, params = condition.to_sql()

        assert isinstance(sql, Composed)
        sql_str = sql.as_string(None)
        assert "data" in sql_str
        assert "'device'" in sql_str
        assert "'name'" in sql_str
        assert "->" in sql_str
        assert "->>" in sql_str
        assert len(params) == 1
        assert params[0] == "Printer"

    def test_sql_column_condition_to_sql(self):
        """Test SQL column condition generates correct SQL."""
        condition = FieldCondition(
            field_path=["status"],
            operator="eq",
            value="active",
            lookup_strategy="sql_column",
            target_column="status",
        )

        sql, params = condition.to_sql()

        assert isinstance(sql, Composed)
        assert "status" in sql.as_string(None)
        assert "=" in sql.as_string(None)
        assert len(params) == 1
        assert params[0] == "active"

    def test_in_operator_to_sql(self):
        """Test IN operator generates correct SQL."""
        condition = FieldCondition(
            field_path=["status"],
            operator="in",
            value=["active", "pending"],
            lookup_strategy="sql_column",
            target_column="status",
        )

        sql, params = condition.to_sql()

        sql_str = sql.as_string(None)
        assert "status" in sql_str
        assert "IN" in sql_str
        assert len(params) == 1
        assert params[0] == ("active", "pending")

    def test_isnull_operator_to_sql(self):
        """Test IS NULL operator generates correct SQL."""
        condition = FieldCondition(
            field_path=["machine_id"],
            operator="isnull",
            value=True,
            lookup_strategy="sql_column",
            target_column="machine_id",
        )

        sql, params = condition.to_sql()

        sql_str = sql.as_string(None)
        assert "machine_id" in sql_str
        assert "IS NULL" in sql_str
        assert len(params) == 0  # IS NULL has no parameters

    def test_contains_operator_to_sql(self):
        """Test LIKE operator for contains generates correct SQL."""
        condition = FieldCondition(
            field_path=["name"],
            operator="contains",
            value="test",
            lookup_strategy="sql_column",
            target_column="name",
        )

        sql, params = condition.to_sql()

        sql_str = sql.as_string(None)
        assert "name" in sql_str
        assert "LIKE" in sql_str
        assert len(params) == 1
        assert params[0] == "%test%"

    def test_repr(self):
        """Test FieldCondition repr is readable."""
        condition = FieldCondition(
            field_path=["machine", "id"],
            operator="eq",
            value="test-value",
            lookup_strategy="fk_column",
            target_column="machine_id",
        )

        repr_str = repr(condition)
        assert "machine.id" in repr_str
        assert "eq" in repr_str
        assert "test-value" in repr_str
        assert "FK:machine_id" in repr_str


class TestWhereClause:
    """Test WhereClause dataclass."""

    def test_create_simple_where_clause(self):
        """Test creating simple WHERE clause with one condition."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                )
            ]
        )

        assert len(clause.conditions) == 1
        assert clause.logical_op == "AND"
        assert len(clause.nested_clauses) == 0

    def test_create_multi_condition_where_clause(self):
        """Test creating WHERE clause with multiple conditions."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
                FieldCondition(
                    field_path=["machine", "id"],
                    operator="eq",
                    value=uuid.UUID("12345678-1234-1234-1234-123456789abc"),
                    lookup_strategy="fk_column",
                    target_column="machine_id",
                ),
            ],
            logical_op="AND"
        )

        assert len(clause.conditions) == 2
        assert clause.logical_op == "AND"

    def test_create_or_where_clause(self):
        """Test creating WHERE clause with OR logic."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="pending",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
            ],
            logical_op="OR"
        )

        assert clause.logical_op == "OR"

    def test_empty_where_clause_raises_error(self):
        """Test empty WHERE clause raises ValueError."""
        with pytest.raises(ValueError, match="must have at least one condition"):
            WhereClause(conditions=[], nested_clauses=[], not_clause=None)

    def test_invalid_logical_op_raises_error(self):
        """Test invalid logical_op raises ValueError."""
        with pytest.raises(ValueError, match="Invalid logical_op"):
            WhereClause(
                conditions=[
                    FieldCondition(
                        field_path=["status"],
                        operator="eq",
                        value="active",
                        lookup_strategy="sql_column",
                        target_column="status",
                    )
                ],
                logical_op="XOR"  # Invalid
            )

    def test_simple_where_clause_to_sql(self):
        """Test simple WHERE clause generates correct SQL."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                )
            ]
        )

        sql, params = clause.to_sql()

        assert sql is not None
        sql_str = sql.as_string(None)
        assert "status" in sql_str
        assert "=" in sql_str
        assert len(params) == 1
        assert params[0] == "active"

    def test_multi_condition_where_clause_to_sql(self):
        """Test multi-condition WHERE clause generates correct SQL."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
                FieldCondition(
                    field_path=["machine", "id"],
                    operator="eq",
                    value=uuid.UUID("12345678-1234-1234-1234-123456789abc"),
                    lookup_strategy="fk_column",
                    target_column="machine_id",
                ),
            ],
            logical_op="AND"
        )

        sql, params = clause.to_sql()

        assert sql is not None
        sql_str = sql.as_string(None)
        assert "status" in sql_str
        assert "machine_id" in sql_str
        assert "AND" in sql_str
        assert len(params) == 2

    def test_or_where_clause_to_sql(self):
        """Test OR WHERE clause generates correct SQL."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="pending",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
            ],
            logical_op="OR"
        )

        sql, params = clause.to_sql()

        sql_str = sql.as_string(None)
        assert "OR" in sql_str

    def test_nested_where_clause_to_sql(self):
        """Test nested WHERE clause generates correct SQL with parentheses."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["machine", "id"],
                    operator="eq",
                    value=uuid.UUID("12345678-1234-1234-1234-123456789abc"),
                    lookup_strategy="fk_column",
                    target_column="machine_id",
                ),
            ],
            nested_clauses=[
                WhereClause(
                    conditions=[
                        FieldCondition(
                            field_path=["status"],
                            operator="eq",
                            value="active",
                            lookup_strategy="sql_column",
                            target_column="status",
                        ),
                        FieldCondition(
                            field_path=["status"],
                            operator="eq",
                            value="pending",
                            lookup_strategy="sql_column",
                            target_column="status",
                        ),
                    ],
                    logical_op="OR"
                )
            ]
        )

        sql, params = clause.to_sql()

        sql_str = sql.as_string(None)
        assert "machine_id" in sql_str
        assert "status" in sql_str
        assert "OR" in sql_str
        assert "(" in sql_str  # Nested clause should be wrapped
        assert ")" in sql_str

    def test_not_clause_to_sql(self):
        """Test NOT clause generates correct SQL."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
            ],
            not_clause=WhereClause(
                conditions=[
                    FieldCondition(
                        field_path=["machine_id"],
                        operator="isnull",
                        value=True,
                        lookup_strategy="sql_column",
                        target_column="machine_id",
                    ),
                ]
            )
        )

        sql, params = clause.to_sql()

        sql_str = sql.as_string(None)
        assert "NOT" in sql_str
        assert "(" in sql_str
        assert "machine_id" in sql_str

    def test_repr(self):
        """Test WhereClause repr is readable."""
        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["status"],
                    operator="eq",
                    value="active",
                    lookup_strategy="sql_column",
                    target_column="status",
                ),
            ]
        )

        repr_str = repr(clause)
        assert "WhereClause" in repr_str
        assert "status" in repr_str
```

### Step 3: Create Tests for Normalization (TDD - These Will Fail)

Create `tests/unit/test_where_normalization.py`:

```python
"""Tests for WHERE clause normalization.

These tests define the expected behavior for converting dict and WhereInput
to canonical WhereClause representation. They will fail initially (RED phase)
and pass once normalization is implemented (GREEN phase).
"""

import uuid

import pytest

from fraiseql.where_clause import WhereClause, FieldCondition
from fraiseql.db import FraiseQLRepository


class TestDictNormalization:
    """Test dict WHERE normalization.

    These tests will FAIL until Phase 2 implements _normalize_dict_where().
    """

    @pytest.mark.skip(reason="Not implemented yet - Phase 2")
    def test_normalize_simple_dict(self):
        """Test normalizing simple dict WHERE clause."""
        where = {"status": {"eq": "active"}}

        # This will fail until we implement normalization
        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"status"}
        )

        assert isinstance(clause, WhereClause)
        assert len(clause.conditions) == 1
        assert clause.conditions[0].field_path == ["status"]
        assert clause.conditions[0].operator == "eq"
        assert clause.conditions[0].value == "active"
        assert clause.conditions[0].lookup_strategy == "sql_column"

    @pytest.mark.skip(reason="Not implemented yet - Phase 2")
    def test_normalize_nested_fk_dict(self):
        """Test normalizing nested FK filter."""
        machine_id = uuid.UUID("12345678-1234-1234-1234-123456789abc")
        where = {"machine": {"id": {"eq": machine_id}}}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"machine_id", "data"}
        )

        assert isinstance(clause, WhereClause)
        assert len(clause.conditions) == 1
        assert clause.conditions[0].field_path == ["machine", "id"]
        assert clause.conditions[0].operator == "eq"
        assert clause.conditions[0].value == machine_id
        assert clause.conditions[0].lookup_strategy == "fk_column"
        assert clause.conditions[0].target_column == "machine_id"

    @pytest.mark.skip(reason="Not implemented yet - Phase 2")
    def test_normalize_nested_jsonb_dict(self):
        """Test normalizing nested JSONB filter."""
        where = {"device": {"name": {"eq": "Printer"}}}

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where,
            view_name="tv_allocation",
            table_columns={"id", "data"}  # No device_id column
        )

        assert isinstance(clause, WhereClause)
        assert len(clause.conditions) == 1
        assert clause.conditions[0].field_path == ["device", "name"]
        assert clause.conditions[0].lookup_strategy == "jsonb_path"
        assert clause.conditions[0].jsonb_path == ["device", "name"]


class TestWhereInputNormalization:
    """Test WhereInput normalization.

    These tests will FAIL until Phase 3 implements WhereInput normalization.
    """

    @pytest.mark.skip(reason="Not implemented yet - Phase 3")
    def test_normalize_whereinput_with_uuid_filter(self):
        """Test normalizing WhereInput with UUIDFilter."""
        from fraiseql.sql import UUIDFilter, create_graphql_where_input
        from tests.regression.test_nested_filter_id_field import Allocation, Machine

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        machine_id = uuid.UUID("12345678-1234-1234-1234-123456789abc")
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=machine_id))
        )

        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            where_input,
            view_name="tv_allocation",
            table_columns={"machine_id", "data"}
        )

        assert isinstance(clause, WhereClause)
        assert len(clause.conditions) == 1
        assert clause.conditions[0].field_path == ["machine", "id"]
        assert clause.conditions[0].operator == "eq"
        assert clause.conditions[0].value == machine_id
        assert clause.conditions[0].lookup_strategy == "fk_column"


class TestNormalizationEquivalence:
    """Test dict and WhereInput produce identical WhereClause.

    These tests will FAIL until both Phase 2 and Phase 3 are complete.
    """

    @pytest.mark.skip(reason="Not implemented yet - Phases 2 & 3")
    def test_dict_and_whereinput_produce_identical_whereclause(self):
        """Test dict and WhereInput normalize to identical WhereClause."""
        from fraiseql.sql import UUIDFilter, create_graphql_where_input
        from tests.regression.test_nested_filter_id_field import Allocation, Machine

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        machine_id = uuid.UUID("12345678-1234-1234-1234-123456789abc")

        # Dict version
        where_dict = {"machine": {"id": {"eq": machine_id}}}

        # WhereInput version
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=machine_id))
        )

        repo = FraiseQLRepository(None)

        clause_dict = repo._normalize_where(
            where_dict,
            view_name="tv_allocation",
            table_columns={"machine_id", "data"}
        )

        clause_input = repo._normalize_where(
            where_input,
            view_name="tv_allocation",
            table_columns={"machine_id", "data"}
        )

        # Should be identical
        assert len(clause_dict.conditions) == len(clause_input.conditions)
        assert clause_dict.conditions[0].field_path == clause_input.conditions[0].field_path
        assert clause_dict.conditions[0].operator == clause_input.conditions[0].operator
        assert clause_dict.conditions[0].value == clause_input.conditions[0].value
        assert clause_dict.conditions[0].lookup_strategy == clause_input.conditions[0].lookup_strategy
```

### Step 4: Add SQL Injection Protection Tests

**CRITICAL SECURITY TEST**

Create `tests/unit/test_where_clause_security.py`:

```python
"""Security tests for WHERE clause SQL generation.

Verifies that malicious input is properly escaped and cannot cause SQL injection.
"""

import pytest
from fraiseql.where_clause import FieldCondition, WhereClause


class TestSQLInjectionProtection:
    """Test SQL injection protection in WHERE clause generation."""

    def test_jsonb_path_sql_injection_protection(self):
        """Verify malicious JSONB paths are escaped."""
        # Attempt SQL injection via JSONB path
        malicious_path = ["device'; DROP TABLE users; --", "name"]

        condition = FieldCondition(
            field_path=malicious_path,
            operator="eq",
            value="test",
            lookup_strategy="jsonb_path",
            target_column="data",
            jsonb_path=malicious_path,
        )

        sql, params = condition.to_sql()
        sql_str = sql.as_string(None)

        # Should be escaped as literal string, not executed as SQL
        assert "DROP TABLE" not in sql_str or "DROP TABLE" in repr(sql_str)
        # Psycopg should escape single quotes
        assert "device'; DROP" not in sql_str or "'device''; DROP" in sql_str

    def test_field_name_sql_injection_protection(self):
        """Verify malicious field names are escaped."""
        malicious_field = "status; DELETE FROM allocations; --"

        condition = FieldCondition(
            field_path=[malicious_field],
            operator="eq",
            value="active",
            lookup_strategy="sql_column",
            target_column=malicious_field,
        )

        sql, params = condition.to_sql()
        sql_str = sql.as_string(None)

        # Identifier() should quote field names
        # Should NOT execute DELETE statement
        assert "DELETE FROM" not in sql_str or '"' in sql_str

    def test_operator_value_sql_injection_protection(self):
        """Verify operator values use parameters, not inline SQL."""
        # Attempt SQL injection via value
        malicious_value = "active' OR '1'='1"

        condition = FieldCondition(
            field_path=["status"],
            operator="eq",
            value=malicious_value,
            lookup_strategy="sql_column",
            target_column="status",
        )

        sql, params = condition.to_sql()
        sql_str = sql.as_string(None)

        # Value should be parameterized (%s), not inline
        assert malicious_value not in sql_str
        assert "%s" in sql_str
        assert params[0] == malicious_value  # Value in params, not SQL

    def test_in_operator_sql_injection_protection(self):
        """Verify IN operator values use parameters."""
        malicious_values = ["active", "pending' OR '1'='1"]

        condition = FieldCondition(
            field_path=["status"],
            operator="in",
            value=malicious_values,
            lookup_strategy="sql_column",
            target_column="status",
        )

        sql, params = condition.to_sql()
        sql_str = sql.as_string(None)

        # Should use %s parameter, not inline values
        assert "OR '1'='1'" not in sql_str
        assert "%s" in sql_str
        assert params[0] == tuple(malicious_values)

    def test_like_pattern_sql_injection_protection(self):
        """Verify LIKE patterns don't allow SQL injection."""
        malicious_pattern = "test%' OR '1'='1"

        condition = FieldCondition(
            field_path=["name"],
            operator="contains",
            value=malicious_pattern,
            lookup_strategy="sql_column",
            target_column="name",
        )

        sql, params = condition.to_sql()
        sql_str = sql.as_string(None)

        # Pattern should be parameterized
        assert "OR '1'='1'" not in sql_str
        assert "%s" in sql_str
        # Pattern wrapped with % for contains
        assert params[0] == f"%{malicious_pattern}%"
```

## Verification Commands

```bash
# Run unit tests for WhereClause (should pass)
uv run pytest tests/unit/test_where_clause.py -v

# Run security tests (MUST PASS)
uv run pytest tests/unit/test_where_clause_security.py -v

# Run normalization tests (should be skipped for now)
uv run pytest tests/unit/test_where_normalization.py -v

# Verify all skipped tests are marked correctly
uv run pytest tests/unit/test_where_normalization.py -v --co

# Check code coverage
uv run pytest tests/unit/test_where_clause.py --cov=fraiseql.where_clause --cov-report=term-missing
```

## Acceptance Criteria

- [ ] `where_clause.py` created with `FieldCondition` and `WhereClause` dataclasses
- [ ] All validation logic implemented (operator, lookup_strategy, etc.)
- [ ] `to_sql()` methods generate correct SQL for all scenarios
- [ ] Unit tests for `WhereClause` pass (20+ tests)
- [ ] **Security tests pass (SQL injection protection verified)**
- [ ] Normalization tests created but skipped (will implement in Phase 2-3)
- [ ] Code coverage >90% for `where_clause.py`
- [ ] Documentation complete (docstrings, examples)
- [ ] Type hints complete and validated with mypy

## DO NOT

- ❌ Implement normalization logic yet (that's Phase 2-3)
- ❌ Modify existing `db.py` code (this phase is additive only)
- ❌ Remove or modify any existing WHERE processing code
- ❌ Change existing test behavior

## Notes

This is a **TDD RED phase**: We define the interface and write tests that will fail. The tests document the expected behavior for future phases.

The `WhereClause` dataclass is the foundation of the entire refactor. Taking time to get this right will make all subsequent phases easier.

## Next Phase

**Phase 2:** Implement dict normalization to convert dict WHERE clauses to `WhereClause`.
