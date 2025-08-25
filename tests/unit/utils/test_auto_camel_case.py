import pytest

"""Tests for automatic camelCase to snake_case conversion."""

from fraiseql.core.ast_parser import FieldPath
from fraiseql.core.translate_query import translate_query
from fraiseql.sql.sql_generator import build_sql_query
from fraiseql.utils.casing import to_snake_case


@pytest.mark.unit
class TestAutoCamelCase:
    """Test suite for automatic camelCase to snake_case conversion."""

    def test_simple_field_conversion(self) -> None:
        """Test conversion of simple camelCase fields to snake_case."""
        # Field paths should already be transformed by extract_flat_paths
        field_paths = [
            FieldPath(alias="firstName", path=["first_name"]),  # path is transformed
            FieldPath(alias="lastName", path=["last_name"]),  # path is transformed
            FieldPath(alias="isActive", path=["is_active"]),  # path is transformed
        ]

        sql = build_sql_query(table="users", field_paths=field_paths, json_output=False)

        sql_str = sql.as_string(None)
        assert "data->>'first_name' AS \"firstName\"" in sql_str  # string field
        assert "data->>'last_name' AS \"lastName\"" in sql_str  # string field
        assert (
            "data->'is_active' AS \"isActive\"" in sql_str
        )  # boolean field, uses -> for type preservation

    def test_nested_field_conversion(self) -> None:
        """Test conversion of nested camelCase fields to snake_case."""
        # Field paths should already be transformed by extract_flat_paths
        field_paths = [
            FieldPath(alias="phoneNumber", path=["user_profile", "phone_number"]),
            FieldPath(alias="streetAddress", path=["user_profile", "address", "street_address"]),
        ]

        sql = build_sql_query(table="users", field_paths=field_paths, json_output=False)

        sql_str = sql.as_string(None)
        assert "data->'user_profile'->>'phone_number' AS \"phoneNumber\"" in sql_str
        assert "data->'user_profile'->'address'->>'street_address' AS \"streetAddress\"" in sql_str

    def test_json_output_with_conversion(self) -> None:
        """Test JSON output with camelCase to snake_case conversion."""
        # Field paths should already be transformed by extract_flat_paths
        field_paths = [
            FieldPath(alias="userId", path=["user_id"]),
            FieldPath(alias="createdAt", path=["created_at"]),
        ]

        sql = build_sql_query(
            table="posts", field_paths=field_paths, json_output=True, typename="Post"
        )

        sql_str = sql.as_string(None)
        assert "jsonb_build_object(" in sql_str
        assert "'userId', data->>'user_id'" in sql_str
        assert "'createdAt', data->>'created_at'" in sql_str
        assert "'__typename', 'Post'" in sql_str

    def test_order_by_conversion(self) -> None:
        """Test ORDER BY with camelCase to snake_case conversion."""
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="createdAt", path=["createdAt"]),
        ]

        sql = build_sql_query(
            table="posts",
            field_paths=field_paths,
            order_by=[("createdAt", "DESC"), ("isPublished", "ASC")],
            auto_camel_case=True,
        )

        sql_str = sql.as_string(None)
        # When auto_camel_case=True, the SQL uses the original camelCase
        assert "ORDER BY data->>'createdAt' DESC, data->>'isPublished' ASC" in sql_str

    def test_group_by_conversion(self) -> None:
        """Test GROUP BY with camelCase to snake_case conversion."""
        field_paths = [FieldPath(alias="authorId", path=["authorId"])]

        sql = build_sql_query(
            table="posts",
            field_paths=field_paths,
            group_by=["authorId", "postStatus"],
            auto_camel_case=True,
        )

        sql_str = sql.as_string(None)
        # When auto_camel_case=True, the SQL uses the original camelCase
        assert "GROUP BY data->>'authorId', data->>'postStatus'" in sql_str

    def test_graphql_query_translation(self) -> None:
        """Test full GraphQL query translation with auto_camel_case."""
        query = """
        query {
            firstName
            lastName
            isActive
            createdAt
        }
        """
        sql = translate_query(query=query, table="users", typename="User", auto_camel_case=True)

        sql_str = sql.as_string(None)
        assert "'firstName', data->>'first_name'" in sql_str  # string field
        assert "'lastName', data->>'last_name'" in sql_str  # string field
        assert (
            "'isActive', data->'is_active'" in sql_str
        )  # boolean field, uses -> for type preservation
        assert "'createdAt', data->>'created_at'" in sql_str  # string field

    def test_disabled_by_default(self) -> None:
        """Test that auto_camel_case is disabled by default."""
        field_paths = [FieldPath(alias="firstName", path=["firstName"])]

        sql = build_sql_query(
            table="users",
            field_paths=field_paths,
            json_output=False,
            # auto_camel_case not specified, should default to False
        )

        sql_str = sql.as_string(None)
        # Without conversion, it should use the original camelCase
        assert "data->>'firstName' AS \"firstName\"" in sql_str

    def test_already_snake_case_fields(self) -> None:
        """Test that already snake_case fields are not affected."""
        field_paths = [
            FieldPath(alias="user_name", path=["user_name"]),
            FieldPath(alias="createdAt", path=["created_at"]),
        ]

        sql = build_sql_query(table="users", field_paths=field_paths, json_output=False)

        sql_str = sql.as_string(None)
        # Should remain unchanged
        assert "data->>'user_name' AS \"user_name\"" in sql_str
        assert "data->>'created_at' AS \"createdAt\"" in sql_str

    def test_special_cases(self) -> None:
        """Test special cases in camelCase to snake_case conversion."""
        # Test the actual to_snake_case function behavior
        assert to_snake_case("apiKey") == "api_key"
        assert to_snake_case("APIKey") == "api_key"  # Consecutive caps handled correctly
        assert to_snake_case("page2Content") == "page2_content"
        assert (
            to_snake_case("HTTPSConnection") == "https_connection"
        )  # Consecutive caps handled correctly
        assert to_snake_case("id") == "id"
        assert to_snake_case("ID") == "id"  # Consecutive caps handled correctly

    def test_complex_nested_query(self) -> None:
        """Test complex nested query with auto_camel_case."""
        query = """
        query {
            id
            title
            isPublished
            publishedAt
            author {
                firstName
                lastName
                emailAddress
            }
            comments {
                commentText
                createdAt
            }
        }
        """
        sql = translate_query(query=query, table="posts", typename="Post", auto_camel_case=True)

        sql_str = sql.as_string(None)
        # Check nested field conversions
        assert (
            "'isPublished', data->'is_published'" in sql_str
        )  # boolean field, uses -> for type preservation
        assert "'publishedAt', data->>'published_at'" in sql_str  # string field
        assert "'firstName', data->'author'->>'first_name'" in sql_str
        assert "'lastName', data->'author'->>'last_name'" in sql_str
        assert "'emailAddress', data->'author'->>'email_address'" in sql_str
        assert "'commentText', data->'comments'->>'comment_text'" in sql_str
        assert "'createdAt', data->'comments'->>'created_at'" in sql_str
