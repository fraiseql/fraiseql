"""Tests for LTREE array matching operators: matches_any_lquery, in_array, array_contains.

This module tests the new array-based matching operators that will be added to
LTreeOperatorStrategy for comprehensive hierarchical path operations.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import LTreeOperatorStrategy
from fraiseql.types import LTree


class TestLTreeArrayOperators:
    """Test LTREE array matching operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_ltree_matches_any_in_array(self):
        """Test ? operator with ltree array - matches any path."""
        # 'top.science.physics' ? ARRAY['top.science.*', 'top.tech.*'] = true
        patterns = ["top.science.*", "top.technology.*"]
        result = self.strategy.build_sql(self.path_sql, "matches_any_lquery", patterns, LTree)
        expected = "(data->>'path')::ltree ? ARRAY['top.science.*', 'top.technology.*']"
        assert result.as_string(None) == expected

    def test_ltree_array_contains_path(self):
        """Test @> operator with path array - array contains path."""
        # ARRAY['top.science', 'top.technology'] @> 'top.science' = true
        paths_array = ["top.science", "top.technology", "top.arts"]
        target_path = "top.science"
        result = self.strategy.build_sql(
            self.path_sql, "array_contains", (paths_array, target_path), LTree
        )
        expected = "ARRAY['top.science'::ltree, 'top.technology'::ltree, 'top.arts'::ltree] @> 'top.science'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_path_in_array(self):
        """Test <@ operator - path is contained in array."""
        # 'top.science' <@ ARRAY['top.science', 'top.technology'] = true
        valid_paths = ["top.science", "top.technology", "top.arts"]
        result = self.strategy.build_sql(self.path_sql, "in_array", valid_paths, LTree)
        expected = "(data->>'path')::ltree <@ ARRAY['top.science'::ltree, 'top.technology'::ltree, 'top.arts'::ltree]"
        assert result.as_string(None) == expected

    def test_matches_any_lquery_single_pattern(self):
        """Test matches_any_lquery with single pattern."""
        patterns = ["*.science.*"]
        result = self.strategy.build_sql(self.path_sql, "matches_any_lquery", patterns, LTree)
        expected = "(data->>'path')::ltree ? ARRAY['*.science.*']"
        assert result.as_string(None) == expected

    def test_in_array_single_path(self):
        """Test in_array with single valid path."""
        valid_paths = ["top.science"]
        result = self.strategy.build_sql(self.path_sql, "in_array", valid_paths, LTree)
        expected = "(data->>'path')::ltree <@ ARRAY['top.science'::ltree]"
        assert result.as_string(None) == expected

    def test_array_contains_single_path(self):
        """Test array_contains with single path in array."""
        paths_array = ["top.science"]
        target_path = "top.science"
        result = self.strategy.build_sql(
            self.path_sql, "array_contains", (paths_array, target_path), LTree
        )
        expected = "ARRAY['top.science'::ltree] @> 'top.science'::ltree"
        assert result.as_string(None) == expected


class TestLTreeArrayValidation:
    """Test validation for array operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_matches_any_lquery_requires_list(self):
        """Test that matches_any_lquery operator requires a list."""
        with pytest.raises(TypeError):
            self.strategy.build_sql(self.path_sql, "matches_any_lquery", "not-a-list", LTree)

    def test_in_array_requires_list(self):
        """Test that in_array operator requires a list."""
        with pytest.raises(TypeError):
            self.strategy.build_sql(self.path_sql, "in_array", "not-a-list", LTree)

    def test_array_contains_requires_tuple(self):
        """Test that array_contains operator requires a tuple (array, target)."""
        with pytest.raises((TypeError, ValueError)):
            self.strategy.build_sql(
                self.path_sql, "array_contains", ["top.science"], LTree
            )  # Not a tuple

    def test_matches_any_lquery_empty_list(self):
        """Test matches_any_lquery with empty list."""
        with pytest.raises((TypeError, ValueError)):
            self.strategy.build_sql(self.path_sql, "matches_any_lquery", [], LTree)

    def test_in_array_empty_list(self):
        """Test in_array with empty list."""
        # Empty array should generate valid SQL
        result = self.strategy.build_sql(self.path_sql, "in_array", [], LTree)
        expected = "(data->>'path')::ltree <@ ARRAY[]"
        assert result.as_string(None) == expected


class TestLTreeArrayEdgeCases:
    """Test edge cases for array operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_matches_any_lquery_with_complex_patterns(self):
        """Test matches_any_lquery with complex lquery patterns."""
        patterns = ["top.*.physics", "*.science.*", "top.arts.music.*"]
        result = self.strategy.build_sql(self.path_sql, "matches_any_lquery", patterns, LTree)
        expected = (
            "(data->>'path')::ltree ? ARRAY['top.*.physics', '*.science.*', 'top.arts.music.*']"
        )
        assert result.as_string(None) == expected

    def test_in_array_with_deep_paths(self):
        """Test in_array with deeply nested paths."""
        valid_paths = [
            "top.academics.university.department.faculty.professor",
            "top.academics.university.department.staff.admin",
            "top.business.company.division.team",
        ]
        result = self.strategy.build_sql(self.path_sql, "in_array", valid_paths, LTree)
        expected = "(data->>'path')::ltree <@ ARRAY['top.academics.university.department.faculty.professor'::ltree, 'top.academics.university.department.staff.admin'::ltree, 'top.business.company.division.team'::ltree]"
        assert result.as_string(None) == expected

    def test_array_contains_with_ancestor_check(self):
        """Test array_contains for checking if array contains ancestor."""
        paths_array = ["top.science", "top.science.physics", "top.science.chemistry"]
        target_path = "top.science"
        result = self.strategy.build_sql(
            self.path_sql, "array_contains", (paths_array, target_path), LTree
        )
        expected = "ARRAY['top.science'::ltree, 'top.science.physics'::ltree, 'top.science.chemistry'::ltree] @> 'top.science'::ltree"
        assert result.as_string(None) == expected

    def test_matches_any_lquery_with_wildcards(self):
        """Test matches_any_lquery with various wildcard patterns."""
        patterns = ["*", "*.science", "top.*", "*.*.physics"]
        result = self.strategy.build_sql(self.path_sql, "matches_any_lquery", patterns, LTree)
        expected = "(data->>'path')::ltree ? ARRAY['*', '*.science', 'top.*', '*.*.physics']"
        assert result.as_string(None) == expected
