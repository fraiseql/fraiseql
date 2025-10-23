"""Comprehensive tests for all existing LTREE operators via LTreeOperatorStrategy.

This module tests the LTreeOperatorStrategy directly to ensure all existing
operators work correctly and expose any edge cases.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import LTreeOperatorStrategy
from fraiseql.types import LTree


class TestExistingLTreeOperators:
    """Verify all current LTREE operators work correctly."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_ltree_eq_operator(self):
        """Test exact path equality."""
        result = self.strategy.build_sql(self.path_sql, "eq", "top.science.physics", LTree)
        expected = "(data->>'path')::ltree = 'top.science.physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_neq_operator(self):
        """Test exact path inequality."""
        result = self.strategy.build_sql(self.path_sql, "neq", "top.technology", LTree)
        expected = "(data->>'path')::ltree != 'top.technology'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_in_operator(self):
        """Test path in list."""
        paths = ["top.science", "top.technology", "top.arts"]
        result = self.strategy.build_sql(self.path_sql, "in", paths, LTree)
        expected = "(data->>'path')::ltree IN ('top.science'::ltree, 'top.technology'::ltree, 'top.arts'::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_notin_operator(self):
        """Test path not in list."""
        paths = ["top.science.physics", "top.science.chemistry"]
        result = self.strategy.build_sql(self.path_sql, "notin", paths, LTree)
        expected = "(data->>'path')::ltree NOT IN ('top.science.physics'::ltree, 'top.science.chemistry'::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_ancestor_of_operator(self):
        """Test @> operator (path1 is ancestor of path2)."""
        # "top.science" @> "top.science.physics" = true
        result = self.strategy.build_sql(self.path_sql, "ancestor_of", "top.science.physics", LTree)
        expected = "(data->>'path')::ltree @> 'top.science.physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_descendant_of_operator(self):
        """Test <@ operator (path1 is descendant of path2)."""
        # "top.science.physics" <@ "top.science" = true
        result = self.strategy.build_sql(self.path_sql, "descendant_of", "top.science", LTree)
        expected = "(data->>'path')::ltree <@ 'top.science'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_matches_lquery(self):
        """Test ~ operator (path matches lquery pattern)."""
        # "top.science.physics" ~ "*.science.*" = true
        result = self.strategy.build_sql(self.path_sql, "matches_lquery", "*.science.*", LTree)
        expected = "(data->>'path')::ltree ~ '*.science.*'::lquery"
        assert result.as_string(None) == expected

    def test_ltree_matches_ltxtquery(self):
        """Test ? operator (path matches ltxtquery text search)."""
        # "top.science.physics" ? "science & physics" = true
        result = self.strategy.build_sql(
            self.path_sql, "matches_ltxtquery", "science & physics", LTree
        )
        expected = "(data->>'path')::ltree ? 'science & physics'::ltxtquery"
        assert result.as_string(None) == expected


class TestLTreeOperatorStrategyValidation:
    """Test LTreeOperatorStrategy validation and error handling."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_can_handle_ltree_specific_operators_without_field_type(self):
        """Test can_handle for LTree-specific operators without field type."""
        # These should work without field type
        assert self.strategy.can_handle("ancestor_of")
        assert self.strategy.can_handle("descendant_of")
        assert self.strategy.can_handle("matches_lquery")
        assert self.strategy.can_handle("matches_ltxtquery")

        # These should NOT work without field type (generic operators)
        assert not self.strategy.can_handle("eq")
        assert not self.strategy.can_handle("neq")
        assert not self.strategy.can_handle("in")
        assert not self.strategy.can_handle("notin")

    def test_can_handle_all_operators_with_ltree_field_type(self):
        """Test can_handle for all operators with LTree field type."""
        # All configured operators should work with LTree type
        for op in self.strategy.operators:
            assert self.strategy.can_handle(op, LTree), f"Should handle {op} with LTree type"

    def test_cannot_handle_unsupported_operators(self):
        """Test that unsupported operators are rejected."""
        assert not self.strategy.can_handle("invalid_op")
        assert not self.strategy.can_handle("invalid_op", LTree)

    def test_rejects_unsupported_pattern_operators(self):
        """Test that pattern operators are explicitly rejected for LTree."""
        with pytest.raises(
            ValueError, match="Pattern operator 'contains' is not supported for LTree fields"
        ):
            self.strategy.build_sql(self.path_sql, "contains", "pattern", LTree)

        with pytest.raises(
            ValueError, match="Pattern operator 'startswith' is not supported for LTree fields"
        ):
            self.strategy.build_sql(self.path_sql, "startswith", "prefix", LTree)

        with pytest.raises(
            ValueError, match="Pattern operator 'endswith' is not supported for LTree fields"
        ):
            self.strategy.build_sql(self.path_sql, "endswith", "suffix", LTree)

    def test_rejects_invalid_field_types(self):
        """Test that non-LTree field types are rejected."""
        with pytest.raises(
            ValueError, match="LTree operator 'eq' can only be used with LTree fields"
        ):
            self.strategy.build_sql(self.path_sql, "eq", "value", str)

    def test_in_operator_requires_list(self):
        """Test that 'in' operator requires a list."""
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            self.strategy.build_sql(self.path_sql, "in", "not-a-list", LTree)

    def test_notin_operator_requires_list(self):
        """Test that 'notin' operator requires a list."""
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            self.strategy.build_sql(self.path_sql, "notin", "not-a-list", LTree)


class TestLTreeOperatorEdgeCases:
    """Test edge cases for LTree operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_empty_lists_for_in_notin(self):
        """Test empty lists for in/notin operators."""
        # Empty IN list
        result_in = self.strategy.build_sql(self.path_sql, "in", [], LTree)
        expected_in = "(data->>'path')::ltree IN ()"
        assert result_in.as_string(None) == expected_in

        # Empty NOT IN list
        result_notin = self.strategy.build_sql(self.path_sql, "notin", [], LTree)
        expected_notin = "(data->>'path')::ltree NOT IN ()"
        assert result_notin.as_string(None) == expected_notin

    def test_single_item_lists(self):
        """Test single item lists for in/notin operators."""
        # Single item IN
        result_in = self.strategy.build_sql(self.path_sql, "in", ["top.science"], LTree)
        expected_in = "(data->>'path')::ltree IN ('top.science'::ltree)"
        assert result_in.as_string(None) == expected_in

        # Single item NOT IN
        result_notin = self.strategy.build_sql(self.path_sql, "notin", ["top.arts"], LTree)
        expected_notin = "(data->>'path')::ltree NOT IN ('top.arts'::ltree)"
        assert result_notin.as_string(None) == expected_notin

    def test_special_characters_in_paths(self):
        """Test paths with underscores and numbers."""
        # Path with underscores
        result = self.strategy.build_sql(self.path_sql, "eq", "top.tech_category.web_dev", LTree)
        expected = "(data->>'path')::ltree = 'top.tech_category.web_dev'::ltree"
        assert result.as_string(None) == expected

        # Path with numbers
        result = self.strategy.build_sql(self.path_sql, "eq", "top.version_2.release_1", LTree)
        expected = "(data->>'path')::ltree = 'top.version_2.release_1'::ltree"
        assert result.as_string(None) == expected

    def test_single_level_paths(self):
        """Test operators with single-level paths."""
        # Single level equality
        result = self.strategy.build_sql(self.path_sql, "eq", "root", LTree)
        expected = "(data->>'path')::ltree = 'root'::ltree"
        assert result.as_string(None) == expected

        # Single level ancestor_of
        result = self.strategy.build_sql(self.path_sql, "ancestor_of", "root.child", LTree)
        expected = "(data->>'path')::ltree @> 'root.child'::ltree"
        assert result.as_string(None) == expected

    def test_deeply_nested_paths(self):
        """Test operators with deeply nested paths."""
        deep_path = "top.academics.university.department.faculty.professor.research.papers"

        # Deep equality
        result = self.strategy.build_sql(self.path_sql, "eq", deep_path, LTree)
        expected = f"(data->>'path')::ltree = '{deep_path}'::ltree"
        assert result.as_string(None) == expected

        # Deep hierarchical relationship
        result = self.strategy.build_sql(self.path_sql, "ancestor_of", deep_path, LTree)
        expected = f"(data->>'path')::ltree @> '{deep_path}'::ltree"
        assert result.as_string(None) == expected
