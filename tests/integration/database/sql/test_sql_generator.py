import pytest

pytestmark = pytest.mark.database

"""Comprehensive tests for sql_generator module to improve coverage."""

from psycopg.sql import SQL

from fraiseql.core.ast_parser import FieldPath
from fraiseql.sql.sql_generator import build_sql_query


@pytest.mark.unit
class TestBuildSqlQuery:
    """Test the build_sql_query function comprehensively."""

    def test_basic_query_no_json_output(self) -> None:
        """Test basic query without JSON output."""
        field_paths = [FieldPath(path=["name"], alias="name"), FieldPath(path=["age"], alias="age")]

        query = build_sql_query("users", field_paths)
        sql_str = query.as_string(None)

        assert "SELECT" in sql_str
        assert "data->>'name' AS \"name\"" in sql_str  # name is string
        assert "data->'age' AS \"age\"" in sql_str  # age is numeric, uses -> for type preservation
        assert 'FROM "users"' in sql_str

    def test_basic_query_with_json_output(self) -> None:
        """Test basic query with JSON output."""
        field_paths = [
            FieldPath(path=["name"], alias="name"),
            FieldPath(path=["email"], alias="email"),
        ]

        query = build_sql_query("users", field_paths, json_output=True)
        sql_str = query.as_string(None)

        assert "jsonb_build_object(" in sql_str
        assert "'name', data->>'name'" in sql_str
        assert "'email', data->>'email'" in sql_str
        assert "AS result" in sql_str

    def test_nested_field_paths(self) -> None:
        """Test query with nested field paths."""
        field_paths = [
            FieldPath(path=["id"], alias="id"),
            FieldPath(path=["profile", "avatar"], alias="avatar"),
            FieldPath(path=["settings", "theme", "color"], alias="themeColor"),
        ]

        query = build_sql_query("users", field_paths, json_output=True)
        sql_str = query.as_string(None)

        assert "data->>'id'" in sql_str
        assert "data->'profile'->>'avatar'" in sql_str
        assert "data->'settings'->'theme'->>'color'" in sql_str

    def test_with_typename(self) -> None:
        """Test query with typename included."""
        field_paths = [FieldPath(path=["id"], alias="id"), FieldPath(path=["name"], alias="name")]

        query = build_sql_query("users", field_paths, json_output=True, typename="User")
        sql_str = query.as_string(None)

        assert "'__typename', 'User'" in sql_str

    def test_with_where_clause(self) -> None:
        """Test query with WHERE clause."""
        field_paths = [FieldPath(path=["name"], alias="name")]
        where_clause = SQL("data->>'is_active' = 'true'")

        query = build_sql_query("users", field_paths, where_clause=where_clause)
        sql_str = query.as_string(None)

        assert "WHERE data->>'is_active' = 'true'" in sql_str

    def test_with_order_by_single_field(self) -> None:
        """Test query with ORDER BY single field."""
        field_paths = [FieldPath(path=["name"], alias="name")]
        order_by = [("name", "ASC")]

        query = build_sql_query("users", field_paths, order_by=order_by)
        sql_str = query.as_string(None)

        assert "ORDER BY data->>'name' ASC" in sql_str

    def test_with_order_by_multiple_fields(self) -> None:
        """Test query with ORDER BY multiple fields."""
        field_paths = [FieldPath(path=["name"], alias="name"), FieldPath(path=["age"], alias="age")]
        order_by = [("age", "DESC"), ("name", "ASC")]

        query = build_sql_query("users", field_paths, order_by=order_by)
        sql_str = query.as_string(None)

        assert "ORDER BY data->>'age' DESC, data->>'name' ASC" in sql_str

    def test_with_order_by_nested_field(self) -> None:
        """Test query with ORDER BY on nested fields."""
        field_paths = [FieldPath(path=["name"], alias="name")]
        order_by = [("profile.created_at", "DESC")]

        query = build_sql_query("users", field_paths, order_by=order_by)
        sql_str = query.as_string(None)

        assert "ORDER BY data->'profile'->>'created_at' DESC" in sql_str

    def test_with_group_by_single_field(self) -> None:
        """Test query with GROUP BY single field."""
        field_paths = [FieldPath(path=["department"], alias="department")]
        group_by = ["department"]

        query = build_sql_query("users", field_paths, group_by=group_by)
        sql_str = query.as_string(None)

        assert "GROUP BY data->>'department'" in sql_str

    def test_with_group_by_multiple_fields(self) -> None:
        """Test query with GROUP BY multiple fields."""
        field_paths = [
            FieldPath(path=["department"], alias="department"),
            FieldPath(path=["role"], alias="role"),
        ]
        group_by = ["department", "role"]

        query = build_sql_query("users", field_paths, group_by=group_by)
        sql_str = query.as_string(None)

        assert "GROUP BY data->>'department', data->>'role'" in sql_str

    def test_with_group_by_nested_field(self) -> None:
        """Test query with GROUP BY on nested fields."""
        field_paths = [FieldPath(path=["name"], alias="name")]
        group_by = ["profile.country", "profile.city"]

        query = build_sql_query("users", field_paths, group_by=group_by)
        sql_str = query.as_string(None)

        assert "GROUP BY data->'profile'->>'country', data->'profile'->>'city'" in sql_str

    def test_with_all_clauses(self) -> None:
        """Test query with WHERE, GROUP BY, and ORDER BY clauses."""
        field_paths = [
            FieldPath(path=["department"], alias="department"),
            FieldPath(path=["count"], alias="count"),
        ]
        where_clause = SQL("data->>'is_active' = 'true'")
        group_by = ["department"]
        order_by = [("count", "DESC")]

        query = build_sql_query(
            "users",
            field_paths,
            where_clause=where_clause,
            group_by=group_by,
            order_by=order_by,
            json_output=True,
        )
        sql_str = query.as_string(None)

        # Check clause order
        where_pos = sql_str.find("WHERE")
        group_pos = sql_str.find("GROUP BY")
        order_pos = sql_str.find("ORDER BY")

        assert where_pos > 0
        assert group_pos > where_pos
        assert order_pos > group_pos

    def test_auto_camel_case_disabled(self) -> None:
        """Test query without auto camel case conversion."""
        field_paths = [FieldPath(path=["user_name"], alias="userName")]

        query = build_sql_query("users", field_paths, auto_camel_case=False)
        sql_str = query.as_string(None)

        # Should use the path as-is
        assert "data->>'user_name'" in sql_str

    def test_auto_camel_case_enabled(self) -> None:
        """Test query with auto camel case conversion."""
        field_paths = [FieldPath(path=["userName"], alias="userName")]
        order_by = [("firstName", "ASC")]
        group_by = ["departmentId"]

        query = build_sql_query(
            "users", field_paths, auto_camel_case=True, order_by=order_by, group_by=group_by
        )
        sql_str = query.as_string(None)

        # When auto_camel_case=True, the SQL uses the original camelCase
        assert "data->>'userName'" in sql_str
        assert "ORDER BY data->>'firstName' ASC" in sql_str
        assert "GROUP BY data->>'departmentId'" in sql_str

    def test_auto_camel_case_with_nested_fields(self) -> None:
        """Test auto camel case with nested field paths."""
        field_paths = [FieldPath(path=["userProfile", "firstName"], alias="firstName")]
        order_by = [("userProfile.lastName", "ASC")]
        group_by = ["userProfile.departmentId"]

        query = build_sql_query(
            "users", field_paths, auto_camel_case=True, order_by=order_by, group_by=group_by
        )
        sql_str = query.as_string(None)

        # When auto_camel_case=True, the SQL uses the original camelCase
        assert "data->'userProfile'->>'firstName'" in sql_str
        assert "ORDER BY data->'userProfile'->>'lastName' ASC" in sql_str
        assert "GROUP BY data->'userProfile'->>'departmentId'" in sql_str

    def test_empty_field_paths(self) -> None:
        """Test query with empty field paths."""
        field_paths = []

        query = build_sql_query("users", field_paths, json_output=True)
        sql_str = query.as_string(None)

        # Should still generate valid SQL
        assert 'SELECT jsonb_build_object() AS result FROM "users"' in sql_str

    def test_special_characters_in_field_names(self) -> None:
        """Test query with special characters in field names."""
        field_paths = [
            FieldPath(path=["field-with-dash"], alias="fieldWithDash"),
            FieldPath(path=["field.with.dots"], alias="fieldWithDots"),
            FieldPath(path=["field with spaces"], alias="fieldWithSpaces"),
        ]

        query = build_sql_query("users", field_paths, json_output=True)
        sql_str = query.as_string(None)

        # Should properly quote field names
        assert "'field-with-dash'" in sql_str
        assert "'field.with.dots'" in sql_str
        assert "'field with spaces'" in sql_str

    def test_complex_nested_structure(self) -> None:
        """Test query with complex nested structure."""
        field_paths = [
            FieldPath(path=["data", "items", "0", "value"], alias="firstItemValue"),
            FieldPath(path=["meta", "tags", "primary"], alias="primaryTag"),
        ]

        query = build_sql_query("documents", field_paths, json_output=True)
        sql_str = query.as_string(None)

        # Should handle numeric indices and deep nesting
        # value is detected as numeric by heuristics, primary as string
        assert "data->'data'->'items'->'0'->'value'" in sql_str
        assert "data->'meta'->'tags'->>'primary'" in sql_str

    def test_table_name_escaping(self) -> None:
        """Test query with table name that needs escaping."""
        field_paths = [FieldPath(path=["id"], alias="id")]

        # Table name with special characters
        query = build_sql_query("user-accounts", field_paths)
        sql_str = query.as_string(None)

        # Should properly escape table name
        assert 'FROM "user-accounts"' in sql_str

    def test_field_alias_different_from_path(self) -> None:
        """Test query where field alias differs from path."""
        field_paths = [
            FieldPath(path=["internal_id"], alias="id"),
            FieldPath(path=["display_name"], alias="name"),
            FieldPath(path=["contact", "email"], alias="emailAddress"),
        ]

        query = build_sql_query("users", field_paths, json_output=True)
        sql_str = query.as_string(None)

        # Check that aliases are used in output
        assert "'id', data->>'internal_id'" in sql_str
        assert "'name', data->>'display_name'" in sql_str
        assert "'emailAddress', data->'contact'->>'email'" in sql_str

    def test_non_json_output_formatting(self) -> None:
        """Test query formatting without JSON output."""
        field_paths = [
            FieldPath(path=["id"], alias="id"),
            FieldPath(path=["profile", "name"], alias="profileName"),
        ]

        query = build_sql_query("users", field_paths, json_output=False)
        sql_str = query.as_string(None)

        # Should format as regular SELECT with aliases
        assert "data->>'id' AS \"id\"" in sql_str
        assert "data->'profile'->>'name' AS \"profileName\"" in sql_str
        assert "jsonb_build_object" not in sql_str


class TestEdgeCasesAndErrors:
    """Test edge cases and error conditions."""

    def test_single_field_query(self) -> None:
        """Test query with single field."""
        field_paths = [FieldPath(path=["id"], alias="id")]

        query = build_sql_query("users", field_paths, json_output=True)
        sql_str = query.as_string(None)

        assert "jsonb_build_object('id', data->>'id') AS result" in sql_str

    def test_very_deep_nesting(self) -> None:
        """Test query with very deep field nesting."""
        field_paths = [
            FieldPath(
                path=["level1", "level2", "level3", "level4", "level5", "value"], alias="deepValue"
            )
        ]

        query = build_sql_query("deep_data", field_paths, json_output=True)
        sql_str = query.as_string(None)

        # value is detected as numeric by heuristics, so uses -> operator
        expected = "data->'level1'->'level2'->'level3'->'level4'->'level5'->'value'"
        assert expected in sql_str

    def test_numeric_field_paths(self) -> None:
        """Test query with numeric indices in field paths."""
        field_paths = [
            FieldPath(path=["items", "0"], alias="firstItem"),
            FieldPath(path=["items", "1", "name"], alias="secondItemName"),
        ]

        query = build_sql_query("arrays", field_paths, json_output=True)
        sql_str = query.as_string(None)

        assert "data->'items'->>'0'" in sql_str
        assert "data->'items'->'1'->>'name'" in sql_str
