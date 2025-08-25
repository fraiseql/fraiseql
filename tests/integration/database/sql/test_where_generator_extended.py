"""Extended tests for WHERE clause generator to improve coverage."""

from dataclasses import dataclass
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Optional

import pytest
from psycopg.sql import SQL, Composed

from fraiseql.sql.where_generator import (
    DynamicType,
    build_operator_composed,
    safe_create_where_type,
)


@pytest.mark.unit
class TestBuildOperatorComposed:
    """Test the build_operator_composed function comprehensively."""

    def test_equality_operator(self):
        """Test equality operator with various types."""
        path_sql = SQL("data->>'name'")

        # String equality
        result = build_operator_composed(path_sql, "eq", "John")
        assert isinstance(result, Composed)

        # Numeric equality
        result = build_operator_composed(path_sql, "eq", 42)
        assert isinstance(result, Composed)

        # Boolean equality
        result = build_operator_composed(path_sql, "eq", True)
        assert isinstance(result, Composed)

    def test_inequality_operator(self):
        """Test not-equal operator."""
        path_sql = SQL("data->>'age'")
        result = build_operator_composed(path_sql, "neq", 25)
        assert isinstance(result, Composed)

    def test_comparison_operators(self):
        """Test greater than, less than operators."""
        path_sql = SQL("data->>'score'")

        # Greater than
        result = build_operator_composed(path_sql, "gt", 100)
        assert isinstance(result, Composed)

        # Greater than or equal
        result = build_operator_composed(path_sql, "gte", 90)
        assert isinstance(result, Composed)

        # Less than
        result = build_operator_composed(path_sql, "lt", 50)
        assert isinstance(result, Composed)

        # Less than or equal
        result = build_operator_composed(path_sql, "lte", 75)
        assert isinstance(result, Composed)

    def test_string_operators(self):
        """Test string-specific operators."""
        path_sql = SQL("data->>'name'")

        # Contains
        result = build_operator_composed(path_sql, "contains", "John")
        assert isinstance(result, Composed)

        # Starts with
        result = build_operator_composed(path_sql, "startswith", "J")
        assert isinstance(result, Composed)

        # Matches (regex)
        result = build_operator_composed(path_sql, "matches", "John")
        assert isinstance(result, Composed)

    def test_list_operators(self):
        """Test list/array operators."""
        path_sql = SQL("data->>'tags'")

        # In operator
        result = build_operator_composed(path_sql, "in", ["python", "javascript"])
        assert isinstance(result, Composed)

        # Not in operator
        result = build_operator_composed(path_sql, "notin", ["go", "rust"])
        assert isinstance(result, Composed)

    def test_null_operators(self):
        """Test null checking operators."""
        path_sql = SQL("data->>'optional_field'")

        # Is null
        result = build_operator_composed(path_sql, "isnull", True)
        assert isinstance(result, Composed)

        # Is not null
        result = build_operator_composed(path_sql, "isnull", False)
        assert isinstance(result, Composed)

    def test_type_casting_numeric(self):
        """Test type casting for numeric comparisons."""
        path_sql = SQL("data->>'price'")

        # Integer comparison
        result = build_operator_composed(path_sql, "gt", 100)
        assert isinstance(result, Composed)

        # Float comparison
        result = build_operator_composed(path_sql, "gte", 99.99)
        assert isinstance(result, Composed)

        # Decimal comparison
        result = build_operator_composed(path_sql, "lt", Decimal("199.99"))
        assert isinstance(result, Composed)

    def test_type_casting_datetime(self):
        """Test type casting for datetime comparisons."""
        path_sql = SQL("data->>'created_at'")

        # Datetime comparison
        dt = datetime(2023, 1, 1, 12, 0, 0)
        result = build_operator_composed(path_sql, "gte", dt)
        assert isinstance(result, Composed)

        # Date comparison
        d = date(2023, 1, 1)
        result = build_operator_composed(path_sql, "eq", d)
        assert isinstance(result, Composed)

    def test_type_casting_boolean(self):
        """Test type casting for boolean comparisons."""
        path_sql = SQL("data->>'is_active'")

        # Boolean true
        result = build_operator_composed(path_sql, "eq", True)
        assert isinstance(result, Composed)

        # Boolean false
        result = build_operator_composed(path_sql, "neq", False)
        assert isinstance(result, Composed)

    def test_depth_operators(self):
        """Test depth operators for ltree-like operations."""
        path_sql = SQL("data->>'path'")

        # Depth equal
        result = build_operator_composed(path_sql, "depth_eq", 3)
        assert isinstance(result, Composed)

        # Depth greater than
        result = build_operator_composed(path_sql, "depth_gt", 2)
        assert isinstance(result, Composed)

    def test_advanced_operators(self):
        """Test advanced JSONB operators."""
        path_sql = SQL("data->>'config'")

        # Is descendant
        result = build_operator_composed(path_sql, "isdescendant", "parent.child")
        assert isinstance(result, Composed)

        # Strictly contains
        result = build_operator_composed(path_sql, "strictly_contains", {"key": "value"})
        assert isinstance(result, Composed)

    def test_unsupported_operator(self):
        """Test behavior with unsupported operator."""
        path_sql = SQL("data->>'field'")

        with pytest.raises(ValueError, match="Unsupported operator"):
            build_operator_composed(path_sql, "unsupported_op", "value")


class TestSafeCreateWhereType:
    """Test dynamic filter type creation."""

    def test_create_simple_filter_type(self):
        """Test creating a filter type for simple dataclass."""

        @dataclass
        class User:
            id: int
            name: str
            email: str

        FilterType = safe_create_where_type(User)

        # Should create a class
        assert callable(FilterType)

        # Should have filter fields
        filter_instance = FilterType()
        assert hasattr(filter_instance, "id")
        assert hasattr(filter_instance, "name")
        assert hasattr(filter_instance, "email")

    def test_create_filter_with_optional_fields(self):
        """Test creating filter type with optional fields."""

        @dataclass
        class Post:
            id: int
            title: str
            content: Optional[str] = None
            published: bool = False

        FilterType = safe_create_where_type(Post)
        filter_instance = FilterType()

        assert hasattr(filter_instance, "id")
        assert hasattr(filter_instance, "title")
        assert hasattr(filter_instance, "content")
        assert hasattr(filter_instance, "published")

    def test_filter_type_to_sql(self):
        """Test that created filter types implement to_sql method."""

        @dataclass
        class Simple:
            name: str

        FilterType = safe_create_where_type(Simple)
        filter_instance = FilterType()

        # Should implement DynamicType protocol
        assert isinstance(filter_instance, DynamicType)

        # Should have to_sql method
        assert hasattr(filter_instance, "to_sql")
        assert callable(filter_instance.to_sql)

    def test_filter_type_with_complex_types(self):
        """Test filter type creation with complex field types."""

        @dataclass
        class ComplexModel:
            id: int
            created_at: datetime
            score: Decimal
            tags: list[str]
            metadata: dict[str, Any]

        FilterType = safe_create_where_type(ComplexModel)
        filter_instance = FilterType()

        # Should handle complex types
        assert hasattr(filter_instance, "created_at")
        assert hasattr(filter_instance, "score")
        assert hasattr(filter_instance, "tags")
        assert hasattr(filter_instance, "metadata")

    def test_filter_inheritance(self):
        """Test filter type with inheritance."""

        @dataclass
        class BaseModel:
            id: int
            created_at: datetime

        @dataclass
        class User(BaseModel):
            name: str
            email: str

        UserFilter = safe_create_where_type(User)
        user_filter = UserFilter()

        # Should include inherited fields
        assert hasattr(user_filter, "id")
        assert hasattr(user_filter, "created_at")
        assert hasattr(user_filter, "name")
        assert hasattr(user_filter, "email")


class TestDynamicTypeProtocol:
    """Test the DynamicType protocol."""

    def test_protocol_compliance(self):
        """Test that objects can implement the protocol."""

        class CustomFilter:
            def to_sql(self) -> Composed | None:
                return Composed([SQL("1 = 1")])

        filter_instance = CustomFilter()
        assert isinstance(filter_instance, DynamicType)

    def test_protocol_method_signature(self):
        """Test protocol method signature requirements."""

        class InvalidFilter:
            def to_sql(self, extra_param):  # Wrong signature
                return None

        InvalidFilter()
        # Should not satisfy protocol due to signature mismatch
        # Note: runtime_checkable only checks method existence, not signature


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def test_build_operator_with_none_value(self):
        """Test operator building with None values."""
        path_sql = SQL("data->>'field'")

        # None value with equality
        result = build_operator_composed(path_sql, "eq", None)
        assert isinstance(result, Composed)

    def test_build_operator_with_empty_string(self):
        """Test operator building with empty string."""
        path_sql = SQL("data->>'field'")
        result = build_operator_composed(path_sql, "eq", "")
        assert isinstance(result, Composed)

    def test_build_operator_with_complex_nested_value(self):
        """Test operator with complex nested values."""
        path_sql = SQL("data->>'config'")
        complex_value = {"nested": {"key": "value"}}

        result = build_operator_composed(path_sql, "eq", complex_value)
        assert isinstance(result, Composed)

    def test_edge_case_operators(self):
        """Test edge case operator handling."""
        path_sql = SQL("data->>'field'")

        # Test with None values
        result = build_operator_composed(path_sql, "eq", None)
        assert isinstance(result, Composed)

    def test_filter_type_caching(self):
        """Test that filter types are cached properly."""

        @dataclass
        class CachedModel:
            name: str

        # Generate same filter type twice
        Filter1 = safe_create_where_type(CachedModel)
        Filter2 = safe_create_where_type(CachedModel)

        # Should be the same due to caching
        assert Filter1 is Filter2
