"""Comprehensive tests for WHERE clause building functionality.

Includes base SQL builder functions and nested object support.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.core.sql_builder import build_where_clause
from fraiseql.sql.where.operators.base_builders import build_comparison_sql, build_in_list_sql


class TestBuildComparisonSQL:
    """Test the generic comparison SQL builder."""

    def test_comparison_no_casting(self):
        """Test comparison with no type casting."""
        path_sql = SQL("data->>'name'")
        result = build_comparison_sql(path_sql, "John", "=")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'name' = 'John'"
        assert "::" not in sql_str

    def test_comparison_both_sides_cast(self):
        """Test comparison with casting on both sides."""
        path_sql = SQL("data->>'birth_date'")
        result = build_comparison_sql(path_sql, "2023-07-15", "=", "date")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'birth_date')::date = '2023-07-15'::date"

    def test_comparison_left_side_only_cast(self):
        """Test comparison with casting on left side only."""
        path_sql = SQL("data->>'port'")
        result = build_comparison_sql(path_sql, 8080, "=", "integer", cast_value=False)
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'port')::integer = 8080"
        assert "::integer" in sql_str
        assert "8080::integer" not in sql_str

    def test_comparison_different_operators(self):
        """Test comparison with different operators."""
        path_sql = SQL("data->>'age'")
        operators = ["=", "!=", ">", ">=", "<", "<="]

        for op in operators:
            result = build_comparison_sql(path_sql, 25, op, "integer", cast_value=False)
            sql_str = result.as_string(None)
            assert f"(data->>'age')::integer {op} 25" == sql_str

    def test_comparison_with_datetime_cast(self):
        """Test comparison with datetime casting."""
        path_sql = SQL("data->>'created_at'")
        result = build_comparison_sql(path_sql, "2023-07-15 10:30:00", "=", "timestamptz")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'created_at')::timestamptz = '2023-07-15 10:30:00'::timestamptz"

    def test_comparison_with_macaddr_cast(self):
        """Test comparison with MAC address casting."""
        path_sql = SQL("data->>'mac_address'")
        result = build_comparison_sql(path_sql, "00:11:22:33:44:55", "=", "macaddr")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"

    def test_comparison_with_inet_cast(self):
        """Test comparison with IP address casting."""
        path_sql = SQL("data->>'ip_address'")
        result = build_comparison_sql(path_sql, "192.168.1.1", "=", "inet")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'ip_address')::inet = '192.168.1.1'::inet"

    def test_comparison_with_ltree_cast(self):
        """Test comparison with ltree casting."""
        path_sql = SQL("data->>'path'")
        result = build_comparison_sql(path_sql, "root.child.leaf", "=", "ltree")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'path')::ltree = 'root.child.leaf'::ltree"

    def test_comparison_with_numeric_cast(self):
        """Test comparison with numeric casting."""
        path_sql = SQL("data->>'score'")
        result = build_comparison_sql(path_sql, 95.5, ">", "numeric")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'score')::numeric > 95.5::numeric"

    def test_comparison_with_uuid_cast(self):
        """Test comparison with UUID casting."""
        path_sql = SQL("data->>'user_id'")
        uuid_val = "550e8400-e29b-41d4-a716-446655440000"
        result = build_comparison_sql(path_sql, uuid_val, "=", "uuid")
        sql_str = result.as_string(None)
        assert f"(data->>'user_id')::uuid = '{uuid_val}'::uuid" == sql_str


class TestBuildInListSQL:
    """Test the generic IN/NOT IN list SQL builder."""

    def test_in_list_no_casting(self):
        """Test IN list with no type casting."""
        path_sql = SQL("data->>'status'")
        values = ["active", "pending", "approved"]
        result = build_in_list_sql(path_sql, values, "IN")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'status' IN ('active', 'pending', 'approved')"

    def test_notin_list_no_casting(self):
        """Test NOT IN list with no type casting."""
        path_sql = SQL("data->>'role'")
        values = ["admin", "superuser"]
        result = build_in_list_sql(path_sql, values, "NOT IN")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'role' NOT IN ('admin', 'superuser')"

    def test_in_list_both_sides_cast(self):
        """Test IN list with casting on both sides."""
        path_sql = SQL("data->>'birth_date'")
        values = ["2023-01-01", "2023-12-31"]
        result = build_in_list_sql(path_sql, values, "IN", "date")
        sql_str = result.as_string(None)
        expected = "(data->>'birth_date')::date IN ('2023-01-01'::date, '2023-12-31'::date)"
        assert expected == sql_str

    def test_in_list_left_side_only_cast(self):
        """Test IN list with casting on left side only."""
        path_sql = SQL("data->>'port'")
        values = [80, 443, 8080]
        result = build_in_list_sql(path_sql, values, "IN", "integer", cast_value=False)
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'port')::integer IN (80, 443, 8080)"
        assert "::integer" in sql_str
        assert "80::integer" not in sql_str

    def test_in_list_single_value(self):
        """Test IN list with single value."""
        path_sql = SQL("data->>'category'")
        values = ["electronics"]
        result = build_in_list_sql(path_sql, values, "IN")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'category' IN ('electronics')"

    def test_in_list_empty_list(self):
        """Test IN list with empty list."""
        path_sql = SQL("data->>'tags'")
        values = []
        result = build_in_list_sql(path_sql, values, "IN")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'tags' IN ()"

    def test_in_list_with_integers_cast(self):
        """Test IN list with integer casting."""
        path_sql = SQL("data->>'user_id'")
        values = [1, 2, 3, 5, 8]
        result = build_in_list_sql(path_sql, values, "IN", "integer", cast_value=False)
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'user_id')::integer IN (1, 2, 3, 5, 8)"

    def test_in_list_with_timestamps_cast(self):
        """Test IN list with timestamp casting."""
        path_sql = SQL("data->>'created_at'")
        values = ["2023-01-01 00:00:00", "2023-12-31 23:59:59"]
        result = build_in_list_sql(path_sql, values, "IN", "timestamptz")
        sql_str = result.as_string(None)
        expected = "(data->>'created_at')::timestamptz IN ('2023-01-01 00:00:00'::timestamptz, '2023-12-31 23:59:59'::timestamptz)"
        assert expected == sql_str

    def test_in_list_with_mac_addresses_cast(self):
        """Test IN list with MAC address casting."""
        path_sql = SQL("data->>'mac'")
        values = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff"]
        result = build_in_list_sql(path_sql, values, "IN", "macaddr")
        sql_str = result.as_string(None)
        expected = "(data->>'mac')::macaddr IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr)"
        assert expected == sql_str

    def test_in_list_requires_list(self):
        """Test that IN list requires a list."""
        path_sql = SQL("data->>'field'")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_in_list_sql(path_sql, "not-a-list", "IN")  # type: ignore[arg-type]

    def test_notin_list_requires_list(self):
        """Test that NOT IN list requires a list."""
        path_sql = SQL("data->>'field'")
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_in_list_sql(path_sql, 42, "NOT IN")  # type: ignore[arg-type]

    def test_in_list_with_special_characters(self):
        """Test IN list with special characters in values."""
        path_sql = SQL("data->>'name'")
        values = ["O'Connor", "Smith-Jones", "user@domain.com"]
        result = build_in_list_sql(path_sql, values, "IN")
        sql_str = result.as_string(None)
        assert "O''Connor" in sql_str  # SQL escaping
        assert "Smith-Jones" in sql_str
        assert "user@domain.com" in sql_str

    def test_in_list_with_large_numbers(self):
        """Test IN list with large numbers."""
        path_sql = SQL("data->>'big_id'")
        values = [9223372036854775807, 9223372036854775806]
        result = build_in_list_sql(path_sql, values, "IN", "bigint", cast_value=False)
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'big_id')::bigint IN (9223372036854775807, 9223372036854775806)"


class TestBaseBuildersEdgeCases:
    """Test edge cases for base builder functions."""

    def test_comparison_with_none_value(self):
        """Test comparison with None value."""
        path_sql = SQL("data->>'optional_field'")
        result = build_comparison_sql(path_sql, None, "IS")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'optional_field' IS NULL"

    def test_in_list_with_none_values(self):
        """Test IN list containing None values."""
        path_sql = SQL("data->>'nullable_field'")
        values = ["value1", None, "value2"]
        result = build_in_list_sql(path_sql, values, "IN")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'nullable_field' IN ('value1', NULL, 'value2')"

    def test_comparison_with_boolean_values(self):
        """Test comparison with boolean values."""
        path_sql = SQL("data->>'is_active'")
        result = build_comparison_sql(path_sql, True, "=")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'is_active' = true"

    def test_in_list_with_boolean_values(self):
        """Test IN list with boolean values."""
        path_sql = SQL("data->>'flags'")
        values = [True, False, True]
        result = build_in_list_sql(path_sql, values, "IN")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'flags' IN (true, false, true)"


class TestNestedObjectWhereBuilder:
    """Test WHERE clause builder with nested object support."""

    def test_flat_where_clause(self) -> None:
        """Test basic flat WHERE clause (existing functionality)."""
        where = {"status": {"eq": "active"}}
        sql = build_where_clause(where)

        assert sql is not None
        assert "data ->> 'status'" in sql.as_string(None)
        assert " = " in sql.as_string(None)

    def test_single_level_nested_where(self) -> None:
        """Test one level of nesting."""
        where = {"machine": {"name": {"eq": "Machine 1"}}}
        sql = build_where_clause(where)

        # Should generate: data -> 'machine' ->> 'name' = 'Machine 1'
        sql_str = sql.as_string(None)
        assert "data -> 'machine' ->> 'name'" in sql_str
        assert " = " in sql_str

    def test_two_level_nested_where(self) -> None:
        """Test two levels of nesting."""
        where = {"location": {"address": {"city": {"eq": "Seattle"}}}}
        sql = build_where_clause(where)

        # Should generate: data -> 'location' -> 'address' ->> 'city' = 'Seattle'
        sql_str = sql.as_string(None)
        assert "data -> 'location' -> 'address' ->> 'city'" in sql_str

    def test_multiple_nested_conditions(self) -> None:
        """Test multiple conditions at different nesting levels."""
        where = {
            "status": {"eq": "active"},
            "machine": {"name": {"eq": "Machine 1"}, "type": {"eq": "Server"}},
        }
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data ->> 'status'" in sql_str
        assert "data -> 'machine' ->> 'name'" in sql_str
        assert "data -> 'machine' ->> 'type'" in sql_str
        assert " AND " in sql_str

    def test_mixed_operators_nested(self) -> None:
        """Test different operators on nested objects."""
        where = {"machine": {"power": {"gte": 100}, "status": {"neq": "offline"}}}
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data -> 'machine' ->> 'power'" in sql_str
        assert " >= " in sql_str
        assert "data -> 'machine' ->> 'status'" in sql_str
        assert " != " in sql_str

    def test_empty_where_dict(self) -> None:
        """Test empty WHERE dict returns TRUE."""
        where = {}
        sql = build_where_clause(where)
        assert "TRUE" in sql.as_string(None)

    def test_none_value_ignored(self) -> None:
        """Test None values are ignored."""
        where = {"status": {"eq": None}}
        sql = build_where_clause(where)
        # None values should be ignored, so we get TRUE
        assert "TRUE" in sql.as_string(None)

    def test_nested_with_list_operator(self) -> None:
        """Test nested object with list operator."""
        where = {"machine": {"tags": {"in": ["server", "production"]}}}
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data -> 'machine' ->> 'tags'" in sql_str
        assert " IN " in sql_str

    def test_deeply_nested_three_levels(self) -> None:
        """Test three levels of nesting."""
        where = {"organization": {"department": {"team": {"lead": {"eq": "Alice"}}}}}
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data -> 'organization' -> 'department' -> 'team' ->> 'lead'" in sql_str
