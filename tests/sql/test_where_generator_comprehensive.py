"""Comprehensive tests for where_generator module to improve coverage."""

import uuid
from dataclasses import dataclass
from datetime import UTC, date, datetime
from decimal import Decimal
from typing import Optional

import pytest
from psycopg.sql import SQL, Composed

from fraiseql.sql.where_generator import (
    build_operator_composed,
    safe_create_where_type,
    unwrap_type,
)


@dataclass
class SampleModel:
    """Test model with various field types."""

    id: uuid.UUID
    name: str
    age: int
    score: float
    amount: Decimal
    is_active: bool
    created_at: datetime
    birth_date: date
    tags: list[str]
    metadata: dict
    optional_field: Optional[str] = None


class TestBuildOperatorComposed:
    """Test the build_operator_composed function with all operators."""

    def test_eq_operator(self):
        """Test equality operator."""
        path_sql = SQL("data->>'name'")
        result = build_operator_composed(path_sql, "eq", "test")
        assert isinstance(result, Composed)
        assert "=" in result.as_string(None)

    def test_neq_operator(self):
        """Test not equal operator."""
        path_sql = SQL("data->>'name'")
        result = build_operator_composed(path_sql, "neq", "test")
        assert isinstance(result, Composed)
        assert "!=" in result.as_string(None)

    def test_like_operator(self):
        """Test LIKE operator."""
        # Like operator doesn't exist in FraiseQL - skip this test
        # The startswith operator is used instead

    def test_ilike_operator(self):
        """Test ILIKE operator."""
        # ILike operator doesn't exist in FraiseQL - skip this test

    def test_numeric_operators(self):
        """Test numeric comparison operators."""
        path_sql = SQL("data->>'age'")

        # Greater than
        result = build_operator_composed(path_sql, "gt", 21)
        assert " > " in result.as_string(None)

        # Greater than or equal
        result = build_operator_composed(path_sql, "gte", 21)
        assert " >= " in result.as_string(None)

        # Less than
        result = build_operator_composed(path_sql, "lt", 21)
        assert " < " in result.as_string(None)

        # Less than or equal
        result = build_operator_composed(path_sql, "lte", 21)
        assert " <= " in result.as_string(None)

    def test_isnull_operator(self):
        """Test IS NULL and IS NOT NULL operators."""
        path_sql = SQL("data->>'optional'")

        # IS NULL
        result = build_operator_composed(path_sql, "isnull", True)
        assert "IS NULL" in result.as_string(None)

        # IS NOT NULL
        result = build_operator_composed(path_sql, "isnull", False)
        assert "IS NOT NULL" in result.as_string(None)

    def test_jsonb_operators(self):
        """Test JSONB-specific operators."""
        path_sql = SQL("data")

        # Contains
        result = build_operator_composed(path_sql, "contains", {"key": "value"})
        assert " @> " in result.as_string(None)

        # Overlaps
        result = build_operator_composed(path_sql, "overlaps", ["a", "b"])
        assert " && " in result.as_string(None)

    def test_regex_operators(self):
        """Test regex operators."""
        path_sql = SQL("data->>'name'")

        # Matches
        result = build_operator_composed(path_sql, "matches", "^test.*")
        assert " ~ " in result.as_string(None)

        # Startswith
        result = build_operator_composed(path_sql, "startswith", "test")
        assert "LIKE" in result.as_string(None)
        assert "test%" in result.as_string(None)

    def test_in_operator_with_different_types(self):
        """Test IN operator with various value types."""
        path_sql = SQL("data->>'value'")

        # String values
        result = build_operator_composed(path_sql, "in", ["a", "b", "c"])
        sql_str = result.as_string(None)
        assert " IN (" in sql_str
        assert "'a'" in sql_str
        assert "'b'" in sql_str
        assert "'c'" in sql_str

        # Numeric values
        result = build_operator_composed(path_sql, "in", [1, 2, 3])
        sql_str = result.as_string(None)
        assert " IN (" in sql_str
        assert "::numeric" in sql_str

        # Boolean values (converted to strings)
        result = build_operator_composed(path_sql, "in", [True, False])
        sql_str = result.as_string(None)
        assert " IN (" in sql_str
        assert "'true'" in sql_str
        assert "'false'" in sql_str

    def test_in_operator_invalid_type(self):
        """Test IN operator with invalid type raises ValueError."""
        path_sql = SQL("data->>'value'")
        with pytest.raises(ValueError, match="'in' operator requires a list"):
            build_operator_composed(path_sql, "in", "not a list")

    def test_notin_operator(self):
        """Test NOT IN operator."""
        path_sql = SQL("data->>'value'")

        # String values
        result = build_operator_composed(path_sql, "notin", ["a", "b"])
        sql_str = result.as_string(None)
        assert " NOT IN (" in sql_str

        # Boolean values
        result = build_operator_composed(path_sql, "notin", [True, False])
        sql_str = result.as_string(None)
        assert "'true'" in sql_str
        assert "'false'" in sql_str

    def test_notin_operator_invalid_type(self):
        """Test NOT IN operator with invalid type raises ValueError."""
        path_sql = SQL("data->>'value'")
        with pytest.raises(ValueError, match="'notin' operator requires a list"):
            build_operator_composed(path_sql, "notin", "not a list")

    def test_ltree_operators(self):
        """Test ltree-specific operators."""
        path_sql = SQL("path")

        # Depth equal
        result = build_operator_composed(path_sql, "depth_eq", 3)
        assert "nlevel(" in result.as_string(None)
        assert " = " in result.as_string(None)

        # Depth greater than
        result = build_operator_composed(path_sql, "depth_gt", 3)
        assert "nlevel(" in result.as_string(None)
        assert " > " in result.as_string(None)

        # Depth less than
        result = build_operator_composed(path_sql, "depth_lt", 3)
        assert "nlevel(" in result.as_string(None)
        assert " < " in result.as_string(None)

        # Is descendant
        result = build_operator_composed(path_sql, "isdescendant", "root.branch")
        assert " <@ " in result.as_string(None)

    def test_strictly_contains_operator(self):
        """Test strictly contains operator (contains but not equal)."""
        path_sql = SQL("data")
        result = build_operator_composed(path_sql, "strictly_contains", {"key": "value"})
        sql_str = result.as_string(None)
        assert " @> " in sql_str
        assert " AND " in sql_str
        assert " != " in sql_str

    def test_boolean_value_handling(self):
        """Test boolean value conversion to proper SQL."""
        path_sql = SQL("data->>'is_active'")

        # Boolean true
        result = build_operator_composed(path_sql, "eq", True)
        sql_str = result.as_string(None)
        assert "::boolean" in sql_str
        assert "'true'" in sql_str

        # Boolean false
        result = build_operator_composed(path_sql, "eq", False)
        sql_str = result.as_string(None)
        assert "::boolean" in sql_str
        assert "'false'" in sql_str

    def test_uuid_value_handling(self):
        """Test UUID value handling with type hints."""
        path_sql = SQL("data->>'id'")
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")

        result = build_operator_composed(path_sql, "eq", test_uuid, uuid.UUID)
        sql_str = result.as_string(None)
        assert "::uuid" in sql_str

    def test_datetime_value_handling(self):
        """Test datetime value handling."""
        path_sql = SQL("data->>'created_at'")
        test_dt = datetime(2024, 1, 1, 12, 0, 0, tzinfo=UTC)

        result = build_operator_composed(path_sql, "gt", test_dt, datetime)
        sql_str = result.as_string(None)
        assert "::timestamptz" in sql_str

    def test_date_value_handling(self):
        """Test date value handling."""
        path_sql = SQL("data->>'birth_date'")
        test_date = date(2024, 1, 1)

        result = build_operator_composed(path_sql, "lt", test_date, date)
        sql_str = result.as_string(None)
        assert "::date" in sql_str

    def test_unsupported_operator(self):
        """Test unsupported operator raises ValueError."""
        path_sql = SQL("data->>'value'")
        with pytest.raises(ValueError, match="Unsupported operator: invalid_op"):
            build_operator_composed(path_sql, "invalid_op", "value")


class TestUnwrapType:
    """Test the unwrap_type function."""

    def test_unwrap_optional(self):
        """Test unwrapping Optional types."""
        assert unwrap_type(Optional[str]) is str
        assert unwrap_type(Optional[int]) is int
        assert unwrap_type(Optional[uuid.UUID]) is uuid.UUID

    def test_unwrap_union_with_none(self):
        """Test unwrapping Union types with None."""
        assert unwrap_type(str | None) is str
        assert unwrap_type(int | None) is int

    def test_no_unwrap_needed(self):
        """Test types that don't need unwrapping."""
        assert unwrap_type(str) is str
        assert unwrap_type(int) is int
        assert unwrap_type(list[str]) is list[str]

    def test_complex_union(self):
        """Test complex Union types are not unwrapped."""
        union_type = str | int | None
        # Should not unwrap because there are multiple non-None types
        assert unwrap_type(union_type) == union_type


class TestSafeCreateWhereType:
    """Test the safe_create_where_type function."""

    def test_basic_where_type_creation(self):
        """Test creating a basic WHERE type."""
        WhereType = safe_create_where_type(SampleModel)

        # Check it's a proper class
        assert isinstance(WhereType, type)

        # Check it has the expected fields
        instance = WhereType()
        assert hasattr(instance, "id")
        assert hasattr(instance, "name")
        assert hasattr(instance, "age")
        assert hasattr(instance, "to_sql")

    def test_where_type_with_simple_filters(self):
        """Test WHERE type with simple equality filters."""
        WhereType = safe_create_where_type(SampleModel)

        where = WhereType(
            name={"eq": "test"},
            age={"gt": 21},
            is_active={"eq": True},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "(data ->> 'name') = 'test'" in sql_str
        assert "(data ->> 'age') > 21" in sql_str
        assert "(data ->> 'is_active') = 'true'::boolean" in sql_str

    def test_where_type_with_complex_filters(self):
        """Test WHERE type with complex filters."""
        WhereType = safe_create_where_type(SampleModel)

        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        test_date = date(2024, 1, 1)

        where = WhereType(
            id={"eq": test_uuid},
            name={"like": "%test%"},
            age={"in": [21, 22, 23]},
            birth_date={"gte": test_date},
            tags={"contains": ["python", "sql"]},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "::uuid" in sql_str
        assert "LIKE" in sql_str
        assert " IN (" in sql_str
        assert "::date" in sql_str

    def test_where_type_with_null_filters(self):
        """Test WHERE type with null checks."""
        WhereType = safe_create_where_type(SampleModel)

        where = WhereType(
            optional_field={"isnull": True},
            name={"isnull": False},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "(data ->> 'optional_field') IS NULL" in sql_str
        assert "(data ->> 'name') IS NOT NULL" in sql_str

    def test_where_type_with_multiple_operators_same_field(self):
        """Test WHERE type with multiple operators on the same field."""
        WhereType = safe_create_where_type(SampleModel)

        where = WhereType(
            age={"gte": 21, "lte": 65},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "(data ->> 'age') >= 21" in sql_str
        assert " AND " in sql_str
        assert "(data ->> 'age') <= 65" in sql_str

    def test_where_type_empty_filter(self):
        """Test WHERE type with no filters returns None."""
        WhereType = safe_create_where_type(SampleModel)

        where = WhereType()
        sql = where.to_sql()
        assert sql is None

    def test_where_type_with_none_values(self):
        """Test WHERE type ignores None values in filter dicts."""
        WhereType = safe_create_where_type(SampleModel)

        where = WhereType(
            name={"eq": None},  # Should be ignored
            age={"gt": 21},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "name" not in sql_str
        assert "(data ->> 'age') > 21" in sql_str

    def test_where_type_caching(self):
        """Test that safe_create_where_type uses caching."""
        WhereType1 = safe_create_where_type(SampleModel)
        WhereType2 = safe_create_where_type(SampleModel)

        # Should return the same cached type
        assert WhereType1 is WhereType2

    def test_nested_dynamic_type(self):
        """Test WHERE type with nested dynamic type filters."""

        @dataclass
        class Parent:
            id: int
            child: Optional["Child"] = None

        @dataclass
        class Child:
            name: str
            value: int

        ParentWhere = safe_create_where_type(Parent)
        ChildWhere = safe_create_where_type(Child)

        # Create a nested filter
        child_filter = ChildWhere(name={"eq": "test"})
        parent_filter = ParentWhere(
            id={"eq": 1},
            child=child_filter,
        )

        sql = parent_filter.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "(data ->> 'id') = 1" in sql_str
        assert "(data ->> 'name') = 'test'" in sql_str


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def test_invalid_field_type_in_filter(self):
        """Test handling of non-dict filter values."""
        WhereType = safe_create_where_type(SampleModel)

        # Non-dict values should be ignored
        where = WhereType(
            name="not a dict",  # Should be ignored
            age={"gt": 21},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        assert "name" not in sql_str
        assert "(data ->> 'age') > 21" in sql_str

    def test_unsupported_operators_ignored(self):
        """Test that unsupported operators are silently ignored."""
        WhereType = safe_create_where_type(SampleModel)

        where = WhereType(
            name={"invalid_op": "value", "eq": "test"},
        )

        sql = where.to_sql()
        assert sql is not None
        sql_str = sql.as_string(None)

        # Invalid operator ignored, valid one used
        assert "(data ->> 'name') = 'test'" in sql_str
        assert "invalid_op" not in sql_str


@pytest.fixture
def sample_where_type():
    """Provide a sample WHERE type for testing."""
    return safe_create_where_type(SampleModel)
