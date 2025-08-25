import pytest
from graphql import GraphQLError
from psycopg.sql import SQL

from fraiseql.core.translate_query import translate_query


@pytest.mark.unit
class TestTranslateQuery:
    """Test suite for translate_query function."""

    def test_translate_simple_query(self) -> None:
        """Test translating a simple GraphQL query."""
        query = """
        {
            id
            name
            email
        }
        """
        result = translate_query(query=query, table="users", typename="User")

        # Check it returns a Composed SQL object
        assert result is not None
        sql_str = result.as_string(None)

        # Check key components
        assert "SELECT" in sql_str
        assert "jsonb_build_object" in sql_str
        assert "'id', data->>'id'" in sql_str
        assert "'name', data->>'name'" in sql_str
        assert "'email', data->>'email'" in sql_str
        assert "'__typename', 'User'" in sql_str
        assert 'FROM "users"' in sql_str

    def test_translate_query_with_nested_fields(self) -> None:
        """Test translating query with nested fields."""
        query = """
        {
            id
            profile {
                firstName
                lastName
                address {
                    city
                    country
                }
            }
        }
        """
        result = translate_query(query=query, table="users", typename="User")

        sql_str = result.as_string(None)

        # Check nested field paths
        assert "data->'profile'->>'firstName'" in sql_str
        assert "data->'profile'->>'lastName'" in sql_str
        assert "data->'profile'->'address'->>'city'" in sql_str
        assert "data->'profile'->'address'->>'country'" in sql_str

    def test_translate_query_with_where_clause(self) -> None:
        """Test translating query with WHERE clause."""
        where_clause = SQL("data->>'active' = 'true'")

        query = """
        {
            id
            email
        }
        """
        result = translate_query(
            query=query, table="users", typename="User", where_clause=where_clause
        )

        sql_str = result.as_string(None)

        # Check WHERE clause is included
        assert "WHERE" in sql_str
        assert "data->>'active' = 'true'" in sql_str

    def test_translate_query_with_order_by(self) -> None:
        """Test translating query with ORDER BY."""
        query = """
        {
            id
            name
        }
        """
        result = translate_query(
            query=query,
            table="products",
            typename="Product",
            order_by=[("created_at", "desc"), ("name", "asc")],
        )

        sql_str = result.as_string(None)

        # Check ORDER BY is included
        assert "ORDER BY data->>'created_at' DESC, data->>'name' ASC" in sql_str

    def test_translate_query_with_group_by(self) -> None:
        """Test translating query with GROUP BY."""
        query = """
        {
            category
            count
        }
        """
        result = translate_query(
            query=query, table="products", typename="ProductGroup", group_by=["category"]
        )

        sql_str = result.as_string(None)

        # Check GROUP BY is included
        assert "GROUP BY data->>'category'" in sql_str

    def test_translate_query_without_typename(self) -> None:
        """Test translating query without typename."""
        query = """
        {
            id
            name
        }
        """
        result = translate_query(query=query, table="items", typename=None)

        sql_str = result.as_string(None)

        # Should not include __typename field
        assert "'__typename'" not in sql_str

    def test_translate_query_with_special_characters(self) -> None:
        """Test translating query with special table name."""
        query = """
        {
            id
        }
        """
        result = translate_query(
            query=query,
            table="user-accounts",
            typename="UserAccount",  # Table name with hyphen
        )

        sql_str = result.as_string(None)

        # Table name should be properly quoted
        assert '"user-accounts"' in sql_str

    def test_translate_query_with_all_clauses(self) -> None:
        """Test translating query with all optional clauses."""
        # Only WHERE clause is supported in current implementation
        where_clause = SQL("data->>'status' = 'active'")

        query = """
        {
            department
            employeeCount
        }
        """
        result = translate_query(
            query=query, table="employees", typename="DepartmentStats", where_clause=where_clause
        )

        sql_str = result.as_string(None)

        # Check WHERE clause is present
        assert "WHERE" in sql_str
        assert "data->>'status' = 'active'" in sql_str

    def test_translate_invalid_graphql_query(self) -> None:
        """Test that invalid GraphQL query raises error."""
        invalid_query = """
        {
            id
            name {  # Missing closing brace
        }
        """
        with pytest.raises(GraphQLError):
            translate_query(query=invalid_query, table="users", typename="User")

    def test_translate_empty_query(self) -> None:
        """Test translating empty selection set."""
        query = """
        {
        }
        """
        # Empty queries might not be valid GraphQL
        with pytest.raises(GraphQLError):
            translate_query(query=query, table="users", typename="User")

    def test_translate_query_with_nested_order_by(self) -> None:
        """Test translating query with nested field ORDER BY."""
        query = """
        {
            id
            profile {
                name
                age
            }
        }
        """
        result = translate_query(
            query=query,
            table="users",
            typename="User",
            order_by=[("profile.age", "desc"), ("profile.location.city", "asc")],
        )

        sql_str = result.as_string(None)

        # Check nested ORDER BY
        assert (
            """ORDER BY data->'profile'->>'age' DESC, data->'profile'->'location'->>'city' ASC"""
            in sql_str
        )

    def test_translate_query_with_nested_group_by(self) -> None:
        """Test translating query with nested field GROUP BY."""
        query = """
        {
            country
            city
            userCount
        }
        """
        result = translate_query(
            query=query,
            table="analytics",
            typename="LocationStats",
            group_by=["location.country", "location.city"],
        )

        sql_str = result.as_string(None)

        # Check nested GROUP BY
        assert "GROUP BY data->'location'->>'country', data->'location'->>'city'" in sql_str

    def test_translate_query_with_all_features(self) -> None:
        """Test combining WHERE, ORDER BY, and GROUP BY with nested fields."""
        query = """
        {
            browser
            os
            count
        }
        """
        where_clause = SQL("data->'timestamp'::date >= '2024-01-01'")

        result = translate_query(
            query=query,
            table="page_views",
            typename="BrowserStats",
            where_clause=where_clause,
            group_by=["device.browser", "device.os"],
            order_by=[("device.browser", "asc"), ("count", "desc")],
        )

        sql_str = result.as_string(None)

        # Verify all features are present and in correct order
        where_pos = sql_str.find("WHERE")
        group_pos = sql_str.find("GROUP BY")
        order_pos = sql_str.find("ORDER BY")

        assert where_pos > 0
        assert group_pos > where_pos
        assert order_pos > group_pos

        assert "data->'device'->>'browser'" in sql_str
        assert "data->'device'->>'os'" in sql_str
