"""Comprehensive tests for all existing LTREE operators via LTreeOperatorStrategy.

This module tests the LTreeOperatorStrategy directly to ensure all existing
operators work correctly and expose any edge cases.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operators import LTreeOperatorStrategy
from fraiseql.types import LTree


class TestExistingLTreeOperators:
    """Verify all current LTREE operators work correctly."""

    def setup_method(self) -> None:
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_ltree_eq_operator(self) -> None:
        """Test exact path equality."""
        result = self.strategy.build_sql("eq", "top.science.physics", self.path_sql, LTree)
        expected = "(data->>'path')::ltree = 'top.science.physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_neq_operator(self) -> None:
        """Test exact path inequality."""
        result = self.strategy.build_sql("neq", "top.technology", self.path_sql, LTree)
        expected = "(data->>'path')::ltree != 'top.technology'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_in_operator(self) -> None:
        """Test path in list."""
        paths = ["top.science", "top.technology", "top.arts"]
        result = self.strategy.build_sql("in", paths, self.path_sql, LTree)
        expected = "(data->>'path')::ltree IN ('top.science'::ltree, 'top.technology'::ltree, 'top.arts'::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_notin_operator(self) -> None:
        """Test path not in list."""
        paths = ["top.science.physics", "top.science.chemistry"]
        result = self.strategy.build_sql("notin", paths, self.path_sql, LTree)
        expected = "(data->>'path')::ltree NOT IN ('top.science.physics'::ltree, 'top.science.chemistry'::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_ancestor_of_operator(self) -> None:
        """Test @> operator (path1 is ancestor of path2)."""
        # "top.science" @> "top.science.physics" = true
        result = self.strategy.build_sql("ancestor_of", "top.science.physics", self.path_sql, LTree)
        expected = "(data->>'path')::ltree @> 'top.science.physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_descendant_of_operator(self) -> None:
        """Test <@ operator (path1 is descendant of path2)."""
        # "top.science.physics" <@ "top.science" = true
        result = self.strategy.build_sql("descendant_of", "top.science", self.path_sql, LTree)
        expected = "(data->>'path')::ltree <@ 'top.science'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_matches_lquery(self) -> None:
        """Test ~ operator (path matches lquery pattern)."""
        # "top.science.physics" ~ "*.science.*" = true
        result = self.strategy.build_sql("matches_lquery", "*.science.*", self.path_sql, LTree)
        expected = "(data->>'path')::ltree ~ '*.science.*'::lquery"
        assert result.as_string(None) == expected

    def test_ltree_matches_ltxtquery(self) -> None:
        """Test ? operator (path matches ltxtquery text search)."""
        # "top.science.physics" ? "science & physics" = true
        result = self.strategy.build_sql("matches_ltxtquery", "science & physics", self.path_sql, LTree)
        expected = "(data->>'path')::ltree ? 'science & physics'::ltxtquery"
        assert result.as_string(None) == expected


class TestLTreeOperatorEdgeCases:
    """Test edge cases for LTree operators."""

    def setup_method(self) -> None:
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_empty_lists_for_in_notin(self) -> None:
        """Test empty lists for in/notin operators."""
        # Empty IN list
        result_in = self.strategy.build_sql("in", [], self.path_sql, LTree)
        expected_in = "(data->>'path')::ltree IN ()"
        assert result_in.as_string(None) == expected_in

        # Empty NOT IN list
        result_notin = self.strategy.build_sql("notin", [], self.path_sql, LTree)
        expected_notin = "(data->>'path')::ltree NOT IN ()"
        assert result_notin.as_string(None) == expected_notin

    def test_single_item_lists(self) -> None:
        """Test single item lists for in/notin operators."""
        # Single item IN
        result_in = self.strategy.build_sql("in", ["top.science"], self.path_sql, LTree)
        expected_in = "(data->>'path')::ltree IN ('top.science'::ltree)"
        assert result_in.as_string(None) == expected_in

        # Single item NOT IN
        result_notin = self.strategy.build_sql("notin", ["top.arts"], self.path_sql, LTree)
        expected_notin = "(data->>'path')::ltree NOT IN ('top.arts'::ltree)"
        assert result_notin.as_string(None) == expected_notin

    def test_special_characters_in_paths(self) -> None:
        """Test paths with underscores and numbers."""
        # Path with underscores
        result = self.strategy.build_sql("eq", "top.tech_category.web_dev", self.path_sql, LTree)
        expected = "(data->>'path')::ltree = 'top.tech_category.web_dev'::ltree"
        assert result.as_string(None) == expected

        # Path with numbers
        result = self.strategy.build_sql("eq", "top.version_2.release_1", self.path_sql, LTree)
        expected = "(data->>'path')::ltree = 'top.version_2.release_1'::ltree"
        assert result.as_string(None) == expected

    def test_single_level_paths(self) -> None:
        """Test operators with single-level paths."""
        # Single level equality
        result = self.strategy.build_sql("eq", "root", self.path_sql, LTree)
        expected = "(data->>'path')::ltree = 'root'::ltree"
        assert result.as_string(None) == expected

        # Single level ancestor_of
        result = self.strategy.build_sql("ancestor_of", "root.child", self.path_sql, LTree)
        expected = "(data->>'path')::ltree @> 'root.child'::ltree"
        assert result.as_string(None) == expected

    def test_deeply_nested_paths(self) -> None:
        """Test operators with deeply nested paths."""
        deep_path = "top.academics.university.department.faculty.professor.research.papers"

        # Deep equality
        result = self.strategy.build_sql("eq", deep_path, self.path_sql, LTree)
        expected = f"(data->>'path')::ltree = '{deep_path}'::ltree"
        assert result.as_string(None) == expected

        # Deep hierarchical relationship
        result = self.strategy.build_sql("ancestor_of", deep_path, self.path_sql, LTree)
        expected = f"(data->>'path')::ltree @> '{deep_path}'::ltree"
        assert result.as_string(None) == expected
