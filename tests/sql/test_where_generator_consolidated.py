"""Tests for consolidated WHERE generator with validation."""

from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal

import pytest

from fraiseql.errors.user_friendly import SQLGenerationError
from fraiseql.sql.where_generator_consolidated import (
    ValidationMode,
    WhereGeneratorConfig,
    create_where_type,
)


@dataclass
class User:
    """Test user type."""

    id: int
    email: str
    name: str
    age: int | None = None
    is_active: bool = True
    created_at: datetime = None


@dataclass
class Product:
    """Test product type."""

    id: int
    name: str
    price: Decimal
    tags: list[str] = None


class TestWhereGeneratorBasic:
    """Test basic WHERE generation functionality."""

    def test_simple_equality_filter(self) -> None:
        """Test basic equality filtering."""
        UserWhere = create_where_type(User)
        where = UserWhere(email="test@example.com")

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "data->>'email' = %s" in sql_str
        assert where._get_params() == ["test@example.com"]

    def test_multiple_conditions(self) -> None:
        """Test WHERE with multiple conditions."""
        UserWhere = create_where_type(User)
        where = UserWhere(
            email="test@example.com",
            age_gte=18,
            is_active=True,
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "data->>'email' = %s" in sql_str
        assert "(data->>'age')::int >= %s" in sql_str
        assert "(data->>'is_active')::boolean = %s" in sql_str
        assert where._get_params() == ["test@example.com", 18, True]

    def test_null_handling(self) -> None:
        """Test NULL value handling."""
        UserWhere = create_where_type(User)
        where = UserWhere(age_is_null=True)

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "data->>'age' IS NULL" in sql_str
        assert where._get_params() == []

    def test_empty_filter(self) -> None:
        """Test empty filter returns None."""
        UserWhere = create_where_type(User)
        where = UserWhere()

        assert where.to_sql() is None
        assert where._get_params() == []


class TestWhereGeneratorOperators:
    """Test various WHERE operators."""

    def test_comparison_operators(self) -> None:
        """Test lt, lte, gt, gte operators."""
        ProductWhere = create_where_type(Product)
        where = ProductWhere(
            price_lt=100,
            price_gte=10,
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "(data->>'price')::numeric < %s" in sql_str
        assert "(data->>'price')::numeric >= %s" in sql_str
        assert where._get_params() == [100, 10]

    def test_string_operators(self) -> None:
        """Test string-specific operators."""
        UserWhere = create_where_type(User)
        where = UserWhere(
            name_contains="john",
            email_starts_with="admin",
            name_ends_with="son",
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "data->>'name' ILIKE %s" in sql_str
        assert "data->>'email' ILIKE %s" in sql_str
        assert "%john%" in where._get_params()
        assert "admin%" in where._get_params()
        assert "%son" in where._get_params()

    def test_list_operators(self) -> None:
        """Test list/array operators."""
        ProductWhere = create_where_type(Product)
        where = ProductWhere(
            tags_contains="electronics",
            id_in=[1, 2, 3],
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "data->'tags' @> %s" in sql_str
        assert "(data->>'id')::int = ANY(%s)" in sql_str
        params = where._get_params()
        assert params[0] == '["electronics"]'  # JSON array
        assert params[1] == [1, 2, 3]

    def test_not_operators(self) -> None:
        """Test negation operators."""
        UserWhere = create_where_type(User)
        where = UserWhere(
            email_not="banned@example.com",
            age_not_in=[13, 14, 15],
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None)

        assert "data->>'email' != %s" in sql_str
        assert "NOT ((data->>'age')::int = ANY(%s))" in sql_str


class TestWhereGeneratorValidation:
    """Test input validation in WHERE generation."""

    def test_strict_validation_mode(self) -> None:
        """Test strict validation rejects suspicious input."""
        config = WhereGeneratorConfig(validation_mode=ValidationMode.STRICT)
        UserWhere = create_where_type(User, config=config)

        # Should reject SQL injection attempts
        with pytest.raises(SQLGenerationError, match="SQL injection pattern detected"):
            UserWhere(email="'; DROP TABLE users; --").to_sql()

    def test_lenient_validation_mode(self) -> None:
        """Test lenient mode allows suspicious input with sanitization."""
        config = WhereGeneratorConfig(validation_mode=ValidationMode.LENIENT)
        UserWhere = create_where_type(User, config=config)

        # Should allow but sanitize
        where = UserWhere(email="test'; DROP TABLE users; --")
        where.to_sql()

        # Value should be parameterized safely
        params = where._get_params()
        assert params[0] == "test'; DROP TABLE users; --"

    def test_disabled_validation(self) -> None:
        """Test disabled validation mode."""
        config = WhereGeneratorConfig(validation_mode=ValidationMode.DISABLED)
        UserWhere = create_where_type(User, config=config)

        # Should allow any input
        where = UserWhere(email="<script>alert('xss')</script>")
        where.to_sql()

        params = where._get_params()
        assert params[0] == "<script>alert('xss')</script>"

    def test_field_length_validation(self) -> None:
        """Test field length limits."""
        config = WhereGeneratorConfig(
            validation_mode=ValidationMode.STRICT,
            max_string_length=50,
        )
        UserWhere = create_where_type(User, config=config)

        # Should reject very long strings
        with pytest.raises(SQLGenerationError, match="exceeds maximum length"):
            UserWhere(name="x" * 100).to_sql()

    def test_numeric_validation(self) -> None:
        """Test numeric value validation."""
        config = WhereGeneratorConfig(validation_mode=ValidationMode.STRICT)
        UserWhere = create_where_type(User, config=config)

        # Should reject infinity
        with pytest.raises(SQLGenerationError):
            UserWhere(age=float("inf")).to_sql()


class TestWhereGeneratorTypeMapping:
    """Test type-specific SQL generation."""

    def test_integer_type_casting(self) -> None:
        """Test integer fields get proper casting."""
        UserWhere = create_where_type(User)
        where = UserWhere(age=25)

        sql_str = where.to_sql().as_string(None)
        assert "(data->>'age')::int = %s" in sql_str

    def test_boolean_type_casting(self) -> None:
        """Test boolean fields get proper casting."""
        UserWhere = create_where_type(User)
        where = UserWhere(is_active=True)

        sql_str = where.to_sql().as_string(None)
        assert "(data->>'is_active')::boolean = %s" in sql_str

    def test_decimal_type_casting(self) -> None:
        """Test decimal fields get numeric casting."""
        ProductWhere = create_where_type(Product)
        where = ProductWhere(price=Decimal("99.99"))

        sql_str = where.to_sql().as_string(None)
        assert "(data->>'price')::numeric = %s" in sql_str

    def test_datetime_handling(self) -> None:
        """Test datetime field handling."""
        UserWhere = create_where_type(User)
        where = UserWhere(created_at_gte=datetime(2023, 1, 1, tzinfo=UTC))

        sql_str = where.to_sql().as_string(None)
        assert "(data->>'created_at')::timestamp >= %s" in sql_str


class TestWhereGeneratorConfig:
    """Test configuration options."""

    def test_custom_field_mapping(self) -> None:
        """Test custom field name mapping."""
        config = WhereGeneratorConfig(
            field_mapping={"email": "user_email"},
        )
        UserWhere = create_where_type(User, config=config)
        where = UserWhere(email="test@example.com")

        sql_str = where.to_sql().as_string(None)
        assert "data->>'user_email' = %s" in sql_str

    def test_custom_operators(self) -> None:
        """Test adding custom operators."""

        def case_sensitive_eq(path_sql, value):
            from psycopg.sql import SQL, Literal

            return SQL("{} = {}").format(path_sql, Literal(value))

        config = WhereGeneratorConfig(
            custom_operators={"exact": case_sensitive_eq},
        )
        UserWhere = create_where_type(User, config=config)
        where = UserWhere(email_exact="Test@Example.com")

        sql = where.to_sql()
        assert sql is not None  # Custom operator was applied

    def test_excluded_fields(self) -> None:
        """Test excluding fields from WHERE generation."""
        config = WhereGeneratorConfig(
            excluded_fields={"created_at", "id"},
        )
        UserWhere = create_where_type(User, config=config)

        # These attributes should not exist
        assert not hasattr(UserWhere, "id")
        assert not hasattr(UserWhere, "created_at")
        assert not hasattr(UserWhere, "id_gte")
        assert not hasattr(UserWhere, "created_at_lt")


class TestWhereGeneratorEdgeCases:
    """Test edge cases and error handling."""

    def test_nested_field_access(self) -> None:
        """Test accessing nested JSON fields."""
        config = WhereGeneratorConfig(enable_nested_access=True)

        @dataclass
        class UserProfile:
            id: int
            profile: dict  # Contains nested data

        ProfileWhere = create_where_type(UserProfile, config=config)
        where = ProfileWhere(profile__city="New York")

        sql_str = where.to_sql().as_string(None)
        assert "data->'profile'->>'city' = %s" in sql_str

    def test_or_conditions(self) -> None:
        """Test OR condition support."""
        UserWhere = create_where_type(User)
        where = UserWhere(
            _or=[
                {"email": "admin@example.com"},
                {"name": "Admin User"},
            ],
        )

        sql_str = where.to_sql().as_string(None)
        assert "(" in sql_str  # Should have grouped conditions
        assert "OR" in sql_str

    def test_and_conditions_explicit(self) -> None:
        """Test explicit AND conditions."""
        UserWhere = create_where_type(User)
        where = UserWhere(
            _and=[
                {"age_gte": 18},
                {"age_lt": 65},
            ],
        )

        sql_str = where.to_sql().as_string(None)
        assert "AND" in sql_str

    def test_complex_nested_conditions(self) -> None:
        """Test complex nested AND/OR conditions."""
        UserWhere = create_where_type(User)
        where = UserWhere(
            _or=[
                {
                    "_and": [
                        {"age_gte": 18},
                        {"is_active": True},
                    ],
                },
                {"email_ends_with": "@admin.com"},
            ],
        )

        sql = where.to_sql()
        assert sql is not None  # Complex condition should generate SQL
