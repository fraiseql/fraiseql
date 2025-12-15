"""Comprehensive tests for JSONB and NULL operator SQL building."""

from psycopg.sql import SQL

from fraiseql.sql.where.operators.jsonb import (
    build_contained_by_sql,
    build_contains_sql,
    build_get_path_sql,
    build_get_path_text_sql,
    build_has_all_keys_sql,
    build_has_any_keys_sql,
    build_has_key_sql,
    build_path_exists_sql,
    build_path_match_sql,
    build_strictly_contains_sql,
)
from fraiseql.sql.where.operators.nulls import build_isnull_sql


class TestJSONBKeyOperators:
    """Test JSONB key existence operators."""

    def test_has_key(self):
        """Test JSONB has_key operator (?)."""
        field_sql = SQL("data->>'metadata'")
        result = build_has_key_sql(field_sql, "username")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'metadata' ? 'username'"

    def test_has_any_keys(self):
        """Test JSONB has_any_keys operator (?|)."""
        field_sql = SQL("data->>'metadata'")
        result = build_has_any_keys_sql(field_sql, ["email", "phone", "address"])
        sql_str = result.as_string(None)
        assert sql_str == "data->>'metadata' ?| '{email,phone,address}'"

    def test_has_all_keys(self):
        """Test JSONB has_all_keys operator (?&)."""
        field_sql = SQL("data->>'config'")
        result = build_has_all_keys_sql(field_sql, ["host", "port", "protocol"])
        sql_str = result.as_string(None)
        assert sql_str == "data->>'config' ?& '{host,port,protocol}'"

    def test_has_single_key_any(self):
        """Test has_any_keys with single key."""
        field_sql = SQL("metadata")
        result = build_has_any_keys_sql(field_sql, ["status"])
        sql_str = result.as_string(None)
        assert sql_str == "metadata ?| '{status}'"

    def test_has_single_key_all(self):
        """Test has_all_keys with single key."""
        field_sql = SQL("metadata")
        result = build_has_all_keys_sql(field_sql, ["id"])
        sql_str = result.as_string(None)
        assert sql_str == "metadata ?& '{id}'"


class TestJSONBContainmentOperators:
    """Test JSONB containment operators."""

    def test_contains_dict(self):
        """Test JSONB contains operator (@>) with dict."""
        field_sql = SQL("data->>'settings'")
        result = build_contains_sql(field_sql, {"theme": "dark", "lang": "en"})
        sql_str = result.as_string(None)
        assert "data->>'settings' @>" in sql_str
        assert "::jsonb" in sql_str
        assert '"theme"' in sql_str or "'theme'" in sql_str
        assert "dark" in sql_str

    def test_contains_list(self):
        """Test JSONB contains operator (@>) with list."""
        field_sql = SQL("tags")
        result = build_contains_sql(field_sql, ["python", "testing"])
        sql_str = result.as_string(None)
        assert "tags @>" in sql_str
        assert "::jsonb" in sql_str
        assert "python" in sql_str
        assert "testing" in sql_str

    def test_contained_by_dict(self):
        """Test JSONB contained_by operator (<@) with dict."""
        field_sql = SQL("data->>'partial'")
        result = build_contained_by_sql(field_sql, {"a": 1, "b": 2, "c": 3})
        sql_str = result.as_string(None)
        assert "data->>'partial' <@" in sql_str
        assert "::jsonb" in sql_str

    def test_contained_by_list(self):
        """Test JSONB contained_by operator (<@) with list."""
        field_sql = SQL("selected_items")
        result = build_contained_by_sql(field_sql, ["item1", "item2", "item3", "item4"])
        sql_str = result.as_string(None)
        assert "selected_items <@" in sql_str
        assert "::jsonb" in sql_str

    def test_strictly_contains(self):
        """Test JSONB strictly_contains (contains but not equal)."""
        field_sql = SQL("data")
        result = build_strictly_contains_sql(field_sql, {"key": "value"})
        sql_str = result.as_string(None)
        assert " @> " in sql_str
        assert " AND " in sql_str
        assert " != " in sql_str
        assert "::jsonb" in sql_str


class TestJSONBPathOperators:
    """Test JSONB path operators."""

    def test_path_exists(self):
        """Test JSONPath exists operator (@?)."""
        field_sql = SQL("data")
        result = build_path_exists_sql(field_sql, "$.user.email")
        sql_str = result.as_string(None)
        assert sql_str == "data @? '$.user.email'"

    def test_path_match(self):
        """Test JSONPath match operator (@@)."""
        field_sql = SQL("data")
        result = build_path_match_sql(field_sql, "$.age > 18")
        sql_str = result.as_string(None)
        assert sql_str == "data @@ '$.age > 18'"

    def test_get_path(self):
        """Test JSONB get path operator (#>)."""
        field_sql = SQL("data")
        result = build_get_path_sql(field_sql, ["user", "profile", "name"])
        sql_str = result.as_string(None)
        assert sql_str == "data #> '{user,profile,name}'"

    def test_get_path_text(self):
        """Test JSONB get path as text operator (#>>)."""
        field_sql = SQL("data")
        result = build_get_path_text_sql(field_sql, ["user", "email"])
        sql_str = result.as_string(None)
        assert sql_str == "data #>> '{user,email}'"

    def test_get_path_single_level(self):
        """Test get_path with single level."""
        field_sql = SQL("config")
        result = build_get_path_sql(field_sql, ["version"])
        sql_str = result.as_string(None)
        assert sql_str == "config #> '{version}'"

    def test_get_path_text_deep(self):
        """Test get_path_text with deep path."""
        field_sql = SQL("data")
        result = build_get_path_text_sql(field_sql, ["a", "b", "c", "d", "e"])
        sql_str = result.as_string(None)
        assert sql_str == "data #>> '{a,b,c,d,e}'"


class TestJSONBEdgeCases:
    """Test JSONB operator edge cases."""

    def test_contains_nested_dict(self):
        """Test contains with nested dictionary."""
        field_sql = SQL("data")
        nested = {"user": {"profile": {"name": "John", "age": 30}}}
        result = build_contains_sql(field_sql, nested)
        sql_str = result.as_string(None)
        assert "@>" in sql_str
        assert "::jsonb" in sql_str

    def test_contains_with_null(self):
        """Test contains with null value."""
        field_sql = SQL("data")
        result = build_contains_sql(field_sql, {"key": None})
        sql_str = result.as_string(None)
        assert "@>" in sql_str
        assert "null" in sql_str.lower()

    def test_contains_with_numbers(self):
        """Test contains with numeric values."""
        field_sql = SQL("data")
        result = build_contains_sql(field_sql, {"count": 42, "score": 95.5})
        sql_str = result.as_string(None)
        assert "@>" in sql_str
        assert "42" in sql_str

    def test_contains_with_boolean(self):
        """Test contains with boolean values."""
        field_sql = SQL("data")
        result = build_contains_sql(field_sql, {"enabled": True, "verified": False})
        sql_str = result.as_string(None)
        assert "@>" in sql_str
        assert "true" in sql_str.lower() or "True" in sql_str

    def test_contains_empty_dict(self):
        """Test contains with empty dictionary."""
        field_sql = SQL("data")
        result = build_contains_sql(field_sql, {})
        sql_str = result.as_string(None)
        assert "@>" in sql_str
        assert "{}" in sql_str

    def test_contains_empty_list(self):
        """Test contains with empty list."""
        field_sql = SQL("data")
        result = build_contains_sql(field_sql, [])
        sql_str = result.as_string(None)
        assert "@>" in sql_str
        assert "[]" in sql_str

    def test_jsonpath_complex_expression(self):
        """Test JSONPath with complex expression."""
        field_sql = SQL("data")
        result = build_path_match_sql(
            field_sql, "$.items[*] ? (@.price < 100 && @.available == true)"
        )
        sql_str = result.as_string(None)
        assert "@@" in sql_str
        assert "items" in sql_str


class TestNullOperators:
    """Test NULL checking operators."""

    def test_isnull_true(self):
        """Test IS NULL operator."""
        path_sql = SQL("data->>'optional_field'")
        result = build_isnull_sql(path_sql, True)
        sql_str = result.as_string(None)
        assert sql_str == "data->>'optional_field' IS NULL"

    def test_isnull_false(self):
        """Test IS NOT NULL operator."""
        path_sql = SQL("data->>'required_field'")
        result = build_isnull_sql(path_sql, False)
        sql_str = result.as_string(None)
        assert sql_str == "data->>'required_field' IS NOT NULL"

    def test_isnull_on_simple_column(self):
        """Test IS NULL on simple column name."""
        path_sql = SQL("email")
        result = build_isnull_sql(path_sql, True)
        sql_str = result.as_string(None)
        assert sql_str == "email IS NULL"

    def test_isnotnull_on_simple_column(self):
        """Test IS NOT NULL on simple column name."""
        path_sql = SQL("email")
        result = build_isnull_sql(path_sql, False)
        sql_str = result.as_string(None)
        assert sql_str == "email IS NOT NULL"

    def test_isnull_on_jsonb_path(self):
        """Test IS NULL on JSONB path."""
        path_sql = SQL("data->'user'->'profile'->>'email'")
        result = build_isnull_sql(path_sql, True)
        sql_str = result.as_string(None)
        assert sql_str == "data->'user'->'profile'->>'email' IS NULL"
