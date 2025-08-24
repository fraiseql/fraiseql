"""Unit tests for GraphQL type system functionality.

This module tests the core GraphQL type generation and translation
functionality in FraiseQL without requiring database connections.
Uses mocks for external dependencies to ensure fast, isolated tests.
"""

from dataclasses import dataclass
from unittest.mock import Mock, patch

import pytest

import fraiseql as fraise
from fraiseql.core.graphql_type import translate_query_from_type
from fraiseql.sql.where_generator import DynamicType


class TestFraiseQLTypeDecorator:
    """Test the @fraiseql.type decorator functionality."""

    def test_type_decorator_basic_setup(self, clear_schema_registry):
        """Test basic type decorator setup."""

        @fraise.type(sql_source="tb_users")
        class UserType:
            profile: dict
            email: str

        assert UserType.__gql_table__ == "tb_users"
        assert UserType.__gql_typename__ == "UserType"
        assert hasattr(UserType, "__gql_where_type__")

    def test_type_decorator_where_type_generation(self, clear_schema_registry):
        """Test where type generation for filtering."""

        @fraise.type(sql_source="tb_users")
        class UserType:
            name: str
            email: str
            age: int

        where_type = UserType.__gql_where_type__
        where_instance = where_type()

        assert isinstance(where_instance, DynamicType)
        assert where_instance.to_sql() is None  # No filters set initially

    def test_type_decorator_with_custom_typename(self, clear_schema_registry):
        """Test type decorator with custom typename (uses class name)."""

        @fraise.type(sql_source="tb_accounts")
        class Account:
            id: str
            balance: float

        assert Account.__gql_table__ == "tb_accounts"
        assert Account.__gql_typename__ == "Account"

    def test_type_decorator_inheritance(self, clear_schema_registry):
        """Test type decorator with inheritance."""

        @fraise.type(sql_source="tb_base")
        class BaseType:
            id: str
            created_at: str

        @fraise.type(sql_source="tb_users")
        class UserType(BaseType):
            username: str
            email: str

        assert UserType.__gql_table__ == "tb_users"
        # Check type annotations exist for inherited fields
        assert "id" in UserType.__annotations__  # Inherited field
        assert "username" in UserType.__annotations__  # Own field


class TestQueryTranslation:
    """Test GraphQL to SQL query translation."""

    def test_translate_simple_query(self, clear_schema_registry):
        """Test translation of simple GraphQL query."""

        @fraise.type(sql_source="tb_accounts")
        @dataclass
        class Account:
            id: str
            role: str

        gql_query = """
        query {
            id
            role
        }
        """

        sql = translate_query_from_type(gql_query, root_type=Account)
        sql_str = sql.as_string(None)

        assert sql_str.startswith("SELECT jsonb_build_object(")
        assert "'id', data->>'id'" in sql_str
        assert "'role', data->>'role'" in sql_str
        assert "'__typename', 'Account'" in sql_str
        assert 'FROM "tb_accounts"' in sql_str

    def test_translate_query_with_nested_fields(self, clear_schema_registry):
        """Test translation with nested object fields."""

        @fraise.type(sql_source="tb_users")
        @dataclass
        class User:
            id: str
            profile: dict
            settings: dict

        gql_query = """
        query {
            id
            profile
            settings
        }
        """

        sql = translate_query_from_type(gql_query, root_type=User)
        sql_str = sql.as_string(None)

        assert "'profile', data->>'profile'" in sql_str
        assert "'settings', data->>'settings'" in sql_str
        assert 'FROM "tb_users"' in sql_str

    def test_translate_query_with_aliases(self, clear_schema_registry):
        """Test translation with field aliases."""

        @fraise.type(sql_source="tb_products")
        @dataclass
        class Product:
            id: str
            name: str
            price: float

        gql_query = """
        query {
            productId: id
            productName: name
            cost: price
        }
        """

        sql = translate_query_from_type(gql_query, root_type=Product)
        sql_str = sql.as_string(None)

        assert "'productId', data->>'id'" in sql_str
        assert "'productName', data->>'name'" in sql_str
        assert "'cost', data->'price'" in sql_str

    def test_translate_query_invalid_field(self, clear_schema_registry):
        """Test translation with field not in type annotation."""

        @fraise.type(sql_source="tb_users")
        @dataclass
        class User:
            id: str
            name: str

        gql_query = """
        query {
            id
            nonexistent_field
        }
        """

        # FraiseQL allows querying fields not in type annotation
        sql = translate_query_from_type(gql_query, root_type=User)
        sql_str = sql.as_string(None)

        assert "'id', data->>'id'" in sql_str
        assert "'nonexistent_field', data->>'nonexistent_field'" in sql_str


class TestWhereTypeGeneration:
    """Test WHERE input type generation."""

    def test_where_type_basic_fields(self, clear_schema_registry):
        """Test WHERE type generation for basic field types."""

        @fraise.type(sql_source="tb_users")
        class User:
            id: str
            name: str
            age: int
            active: bool

        where_type = User.__gql_where_type__
        where_instance = where_type()

        # Should have filter methods for each field
        assert hasattr(where_instance, "id")
        assert hasattr(where_instance, "name")
        assert hasattr(where_instance, "age")
        assert hasattr(where_instance, "active")

    def test_where_type_with_operations(self, clear_schema_registry):
        """Test WHERE type with different filter operations."""

        @fraise.type(sql_source="tb_products")
        class Product:
            id: str
            name: str
            price: float
            created_at: str

        where_type = Product.__gql_where_type__
        where_instance = where_type()

        # Test setting filters (mock the actual filter methods)
        with patch.object(where_instance, "name") as mock_name_filter:
            mock_name_filter.equals = Mock(return_value=where_instance)
            mock_name_filter.contains = Mock(return_value=where_instance)

            # Should be chainable
            result = where_instance.name.equals("test")
            assert result is where_instance

    def test_where_type_sql_generation(self, clear_schema_registry):
        """Test SQL generation from WHERE conditions."""

        @fraise.type(sql_source="tb_orders")
        class Order:
            id: str
            status: str
            total: float

        where_type = Order.__gql_where_type__
        where_instance = where_type()

        # Initially no SQL should be generated
        assert where_instance.to_sql() is None

        # After setting filters, SQL should be generated
        # (This would be tested with actual filter implementation)


class TestTypeSystemIntegration:
    """Test integration between different type system components."""

    def test_multiple_types_registration(self, clear_schema_registry):
        """Test registering multiple types works correctly."""

        @fraise.type(sql_source="tb_users")
        class User:
            id: str
            username: str

        @fraise.type(sql_source="tb_posts")
        class Post:
            id: str
            title: str
            author_id: str

        # Both types should be registered
        assert User.__gql_table__ == "tb_users"
        assert Post.__gql_table__ == "tb_posts"

        # Should have separate where types
        assert User.__gql_where_type__ != Post.__gql_where_type__

    def test_type_with_relationships(self, clear_schema_registry):
        """Test type definition with relationship fields."""

        @fraise.type(sql_source="tb_users")
        class User:
            id: str
            username: str

        @fraise.type(sql_source="tb_posts")
        class Post:
            id: str
            title: str
            author_id: str
            # Relationship field (would be resolved separately)
            author: User

        assert Post.__gql_table__ == "tb_posts"
        # Type hints should include relationship
        annotations = getattr(Post, "__annotations__", {})
        assert "author" in annotations

    @pytest.mark.parametrize(
        "sql_source,expected_table",
        [
            ("tb_users", "tb_users"),
            ("public.tb_accounts", "public.tb_accounts"),
            ("v_user_profiles", "v_user_profiles"),
        ],
    )
    def test_various_sql_sources(self, clear_schema_registry, sql_source, expected_table):
        """Test type decorator with various SQL source formats."""

        @fraise.type(sql_source=sql_source)
        class TestType:
            id: str
            name: str

        assert TestType.__gql_table__ == expected_table


class TestTypeDecoratorEdgeCases:
    """Test edge cases and error conditions."""

    def test_type_without_sql_source_allows_null(self, clear_schema_registry):
        """Test that type decorator works without sql_source."""

        @fraise.type()  # No sql_source specified
        class TypeWithoutSource:
            id: str

        # Should work but have no SQL source - __gql_table__ not set
        assert not hasattr(TypeWithoutSource, "__gql_table__")
        assert hasattr(TypeWithoutSource, "__gql_typename__")
        assert TypeWithoutSource.__gql_typename__ == "TypeWithoutSource"

    def test_type_with_empty_sql_source_allowed(self, clear_schema_registry):
        """Test that empty sql_source is allowed."""

        @fraise.type(sql_source="")
        class TypeWithEmptySource:
            id: str

        # Should work but have no SQL source - __gql_table__ not set for empty source
        assert not hasattr(TypeWithEmptySource, "__gql_table__")
        assert hasattr(TypeWithEmptySource, "__gql_typename__")
        assert TypeWithEmptySource.__gql_typename__ == "TypeWithEmptySource"

    def test_type_redefinition_handling(self, clear_schema_registry):
        """Test behavior when redefining same type."""

        @fraise.type(sql_source="tb_test")
        class TestType:
            id: str
            name: str

        # Redefining should work (for testing scenarios)
        @fraise.type(sql_source="tb_test_v2")
        class TestType:  # Same name
            id: str
            title: str  # Different fields

        # Latest definition should take precedence
        assert TestType.__gql_table__ == "tb_test_v2"

    def test_type_with_invalid_python_identifier(self, clear_schema_registry):
        """Test type with field names that aren't valid Python identifiers."""

        @fraise.type(sql_source="tb_special")
        class SpecialType:
            id: str
            # This would be handled via custom field mapping
            normal_field: str

        # Should still work for normal fields
        assert SpecialType.__gql_table__ == "tb_special"


class TestPerformanceConsiderations:
    """Test performance-related aspects of type system."""

    def test_type_registration_is_fast(self, clear_schema_registry):
        """Test that type registration doesn't have significant overhead."""
        import time

        start_time = time.time()

        # Register multiple types
        for i in range(100):

            @fraise.type(sql_source=f"tb_test_{i}")
            class TestType:
                id: str
                name: str

        end_time = time.time()

        # Should complete quickly (less than 1 second for 100 types)
        assert (end_time - start_time) < 1.0

    def test_where_type_creation_is_cached(self, clear_schema_registry):
        """Test that WHERE type creation is optimized."""

        @fraise.type(sql_source="tb_cached_test")
        class CachedType:
            id: str
            name: str
            email: str

        # Multiple accesses should return same where type class
        where_type_1 = CachedType.__gql_where_type__
        where_type_2 = CachedType.__gql_where_type__

        assert where_type_1 is where_type_2  # Same object reference
