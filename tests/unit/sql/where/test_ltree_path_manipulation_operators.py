"""Tests for LTREE path manipulation operators: concat, lca.

This module tests the new path manipulation operators that will be added to
LTreeOperatorStrategy for comprehensive hierarchical path operations.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import LTreeOperatorStrategy
from fraiseql.types import LTree


class TestLTreePathManipulationOperators:
    """Test LTREE path manipulation operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_ltree_concat_operator(self):
        """Test || operator - concatenate paths."""
        # 'top.science' || 'physics.quantum' = 'top.science.physics.quantum'
        suffix = "physics.quantum"
        result = self.strategy.build_sql(self.path_sql, "concat", suffix, LTree)
        expected = "(data->>'path')::ltree || 'physics.quantum'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_concat_single_label(self):
        """Test concatenating single label to path."""
        # 'top.science' || 'physics' = 'top.science.physics'
        result = self.strategy.build_sql(self.path_sql, "concat", "physics", LTree)
        expected = "(data->>'path')::ltree || 'physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_concat_empty_string(self):
        """Test concatenating empty string."""
        # 'top.science' || '' should work
        result = self.strategy.build_sql(self.path_sql, "concat", "", LTree)
        expected = "(data->>'path')::ltree || ''::ltree"
        assert result.as_string(None) == expected

    def test_ltree_lca_operator(self):
        """Test lca(ltree[]) - lowest common ancestor."""
        # lca('top.science.physics', 'top.science.chemistry') = 'top.science'
        paths = ["top.science.physics", "top.science.chemistry", "top.science.biology"]
        result = self.strategy.build_sql(self.path_sql, "lca", paths, LTree)
        expected = "lca(ARRAY['top.science.physics'::ltree, 'top.science.chemistry'::ltree, 'top.science.biology'::ltree])"
        assert result.as_string(None) == expected

    def test_ltree_lca_two_paths(self):
        """Test lca with just two paths."""
        # lca('top.science', 'top.technology') = 'top'
        paths = ["top.science", "top.technology"]
        result = self.strategy.build_sql(self.path_sql, "lca", paths, LTree)
        expected = "lca(ARRAY['top.science'::ltree, 'top.technology'::ltree])"
        assert result.as_string(None) == expected

    def test_ltree_lca_single_path(self):
        """Test lca with single path (edge case)."""
        # lca with single path should still work
        paths = ["top.science.physics"]
        result = self.strategy.build_sql(self.path_sql, "lca", paths, LTree)
        expected = "lca(ARRAY['top.science.physics'::ltree])"
        assert result.as_string(None) == expected


class TestLTreePathManipulationValidation:
    """Test validation for path manipulation operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_lca_requires_list(self):
        """Test that lca operator requires a list of paths."""
        with pytest.raises(TypeError):
            self.strategy.build_sql(self.path_sql, "lca", "not-a-list", LTree)

    def test_lca_empty_list(self):
        """Test lca with empty list."""
        with pytest.raises((TypeError, ValueError)):
            self.strategy.build_sql(self.path_sql, "lca", [], LTree)

    def test_concat_with_none(self):
        """Test concat with None value."""
        # This should work - concatenating None might be allowed
        try:
            result = self.strategy.build_sql(self.path_sql, "concat", None, LTree)
            # If it doesn't raise, check the SQL
            expected = "(data->>'path')::ltree || NULL::ltree"
            assert result.as_string(None) == expected
        except (TypeError, ValueError):
            # If it raises, that's also acceptable
            pass


class TestLTreePathManipulationEdgeCases:
    """Test edge cases for path manipulation operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_concat_with_special_characters(self):
        """Test concat with paths containing underscores and numbers."""
        suffix = "version_2.release_1"
        result = self.strategy.build_sql(self.path_sql, "concat", suffix, LTree)
        expected = "(data->>'path')::ltree || 'version_2.release_1'::ltree"
        assert result.as_string(None) == expected

    def test_lca_with_deep_paths(self):
        """Test lca with deeply nested paths."""
        paths = [
            "top.academics.university.department.faculty.professor.research.papers",
            "top.academics.university.department.faculty.professor.teaching.courses",
            "top.academics.university.department.staff.admin.budget",
        ]
        result = self.strategy.build_sql(self.path_sql, "lca", paths, LTree)
        expected = "lca(ARRAY['top.academics.university.department.faculty.professor.research.papers'::ltree, 'top.academics.university.department.faculty.professor.teaching.courses'::ltree, 'top.academics.university.department.staff.admin.budget'::ltree])"
        assert result.as_string(None) == expected

    def test_lca_with_identical_paths(self):
        """Test lca with identical paths."""
        paths = ["top.science.physics", "top.science.physics", "top.science.physics"]
        result = self.strategy.build_sql(self.path_sql, "lca", paths, LTree)
        expected = "lca(ARRAY['top.science.physics'::ltree, 'top.science.physics'::ltree, 'top.science.physics'::ltree])"
        assert result.as_string(None) == expected

    def test_concat_result_filtering(self):
        """Test that concat can be used in filtering contexts."""
        # This tests that concat returns a proper SQL expression that can be used in WHERE clauses
        suffix = "quantum"
        result = self.strategy.build_sql(self.path_sql, "concat", suffix, LTree)
        sql_str = result.as_string(None)
        # Should be a valid SQL expression
        assert "||" in sql_str
        assert "::ltree" in sql_str
