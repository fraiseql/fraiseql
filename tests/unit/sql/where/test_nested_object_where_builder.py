"""Test nested object WHERE clause building."""

import pytest
from fraiseql.sql.where.core.sql_builder import build_where_clause


class TestNestedObjectWhereBuilder:
    """Test WHERE clause builder with nested object support."""

    def test_flat_where_clause(self):
        """Test basic flat WHERE clause (existing functionality)."""
        where = {"status": {"eq": "active"}}
        sql = build_where_clause(where)

        assert sql is not None
        assert "data ->> 'status'" in sql.as_string(None)
        assert " = " in sql.as_string(None)

    def test_single_level_nested_where(self):
        """Test one level of nesting."""
        where = {"machine": {"name": {"eq": "Machine 1"}}}
        sql = build_where_clause(where)

        # Should generate: data -> 'machine' ->> 'name' = 'Machine 1'
        sql_str = sql.as_string(None)
        assert "data -> 'machine' ->> 'name'" in sql_str
        assert " = " in sql_str

    def test_two_level_nested_where(self):
        """Test two levels of nesting."""
        where = {"location": {"address": {"city": {"eq": "Seattle"}}}}
        sql = build_where_clause(where)

        # Should generate: data -> 'location' -> 'address' ->> 'city' = 'Seattle'
        sql_str = sql.as_string(None)
        assert "data -> 'location' -> 'address' ->> 'city'" in sql_str

    def test_multiple_nested_conditions(self):
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

    def test_mixed_operators_nested(self):
        """Test different operators on nested objects."""
        where = {"machine": {"power": {"gte": 100}, "status": {"neq": "offline"}}}
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data -> 'machine' ->> 'power'" in sql_str
        assert " >= " in sql_str
        assert "data -> 'machine' ->> 'status'" in sql_str
        assert " != " in sql_str

    def test_empty_where_dict(self):
        """Test empty WHERE dict returns TRUE."""
        where = {}
        sql = build_where_clause(where)
        assert "TRUE" in sql.as_string(None)

    def test_none_value_ignored(self):
        """Test None values are ignored."""
        where = {"status": {"eq": None}}
        sql = build_where_clause(where)
        # None values should be ignored, so we get TRUE
        assert "TRUE" in sql.as_string(None)

    def test_nested_with_list_operator(self):
        """Test nested object with list operator."""
        where = {"machine": {"tags": {"in": ["server", "production"]}}}
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data -> 'machine' ->> 'tags'" in sql_str
        assert " IN " in sql_str

    def test_deeply_nested_three_levels(self):
        """Test three levels of nesting."""
        where = {"organization": {"department": {"team": {"lead": {"eq": "Alice"}}}}}
        sql = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data -> 'organization' -> 'department' -> 'team' ->> 'lead'" in sql_str
