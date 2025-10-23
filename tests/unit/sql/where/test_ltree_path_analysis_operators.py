"""Tests for LTREE path analysis operators: nlevel, subpath, index.

This module tests the new path analysis operators that will be added to
LTreeOperatorStrategy for comprehensive hierarchical path querying.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import LTreeOperatorStrategy
from fraiseql.types import LTree


class TestLTreePathAnalysisOperators:
    """Test LTREE path analysis operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_ltree_nlevel_operator(self):
        """Test nlevel(ltree) - returns number of labels in path."""
        # nlevel('top.science.physics') = 3
        result = self.strategy.build_sql(self.path_sql, "nlevel", None, LTree)
        expected = "nlevel((data->>'path')::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_nlevel_eq_operator(self):
        """Test filtering paths by exact depth using nlevel_eq."""
        # Find all paths with exactly 3 levels
        result = self.strategy.build_sql(self.path_sql, "nlevel_eq", 3, LTree)
        expected = "nlevel((data->>'path')::ltree) = 3"
        assert result.as_string(None) == expected

    def test_ltree_nlevel_gt_operator(self):
        """Test filtering paths deeper than specified level."""
        # Find all paths with more than 2 levels
        result = self.strategy.build_sql(self.path_sql, "nlevel_gt", 2, LTree)
        expected = "nlevel((data->>'path')::ltree) > 2"
        assert result.as_string(None) == expected

    def test_ltree_nlevel_gte_operator(self):
        """Test filtering paths at least as deep as specified level."""
        # Find all paths with 4 or more levels
        result = self.strategy.build_sql(self.path_sql, "nlevel_gte", 4, LTree)
        expected = "nlevel((data->>'path')::ltree) >= 4"
        assert result.as_string(None) == expected

    def test_ltree_nlevel_lt_operator(self):
        """Test filtering paths shallower than specified level."""
        # Find all paths with fewer than 3 levels
        result = self.strategy.build_sql(self.path_sql, "nlevel_lt", 3, LTree)
        expected = "nlevel((data->>'path')::ltree) < 3"
        assert result.as_string(None) == expected

    def test_ltree_nlevel_lte_operator(self):
        """Test filtering paths at most as deep as specified level."""
        # Find all paths with 2 or fewer levels
        result = self.strategy.build_sql(self.path_sql, "nlevel_lte", 2, LTree)
        expected = "nlevel((data->>'path')::ltree) <= 2"
        assert result.as_string(None) == expected

    def test_ltree_subpath_operator(self):
        """Test subpath(ltree, offset, len) - extract subpath."""
        # subpath('top.science.physics.quantum', 1, 2) = 'science.physics'
        offset, length = 1, 2
        result = self.strategy.build_sql(self.path_sql, "subpath", (offset, length), LTree)
        expected = "subpath((data->>'path')::ltree, 1, 2)"
        assert result.as_string(None) == expected

    def test_ltree_subpath_from_start(self):
        """Test subpath from start (offset=0)."""
        # subpath('top.science.physics', 0, 2) = 'top.science'
        offset, length = 0, 2
        result = self.strategy.build_sql(self.path_sql, "subpath", (offset, length), LTree)
        expected = "subpath((data->>'path')::ltree, 0, 2)"
        assert result.as_string(None) == expected

    def test_ltree_subpath_single_element(self):
        """Test subpath extracting single element."""
        # subpath('top.science.physics', 2, 1) = 'physics'
        offset, length = 2, 1
        result = self.strategy.build_sql(self.path_sql, "subpath", (offset, length), LTree)
        expected = "subpath((data->>'path')::ltree, 2, 1)"
        assert result.as_string(None) == expected

    def test_ltree_index_operator(self):
        """Test index(ltree, ltree) - position of sublabel."""
        # index('top.science.physics', 'science') = 1
        sublabel = "science"
        result = self.strategy.build_sql(self.path_sql, "index", sublabel, LTree)
        expected = "index((data->>'path')::ltree, 'science'::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_index_eq_operator(self):
        """Test filtering by exact position of sublabel."""
        # Filter paths where 'science' appears at position 1
        sublabel, position = "science", 1
        result = self.strategy.build_sql(self.path_sql, "index_eq", (sublabel, position), LTree)
        expected = "index((data->>'path')::ltree, 'science'::ltree) = 1"
        assert result.as_string(None) == expected

    def test_ltree_index_gte_operator(self):
        """Test filtering by minimum position of sublabel."""
        # Filter paths where 'physics' appears at position 2 or later
        sublabel, min_position = "physics", 2
        result = self.strategy.build_sql(
            self.path_sql, "index_gte", (sublabel, min_position), LTree
        )
        expected = "index((data->>'path')::ltree, 'physics'::ltree) >= 2"
        assert result.as_string(None) == expected


class TestLTreePathAnalysisValidation:
    """Test validation for path analysis operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_subpath_requires_tuple(self):
        """Test that subpath operator requires a tuple (offset, length)."""
        with pytest.raises((TypeError, ValueError)):
            self.strategy.build_sql(self.path_sql, "subpath", 1, LTree)  # Not a tuple

    def test_subpath_tuple_validation(self):
        """Test subpath tuple validation."""
        # Valid tuple should work (will fail because operator not implemented yet)
        try:
            self.strategy.build_sql(self.path_sql, "subpath", (1, 2), LTree)
        except ValueError as e:
            # Expected to fail in RED phase - operator not implemented
            assert "Unsupported LTree operator" in str(e)

    def test_index_eq_requires_tuple(self):
        """Test that index_eq operator requires a tuple (sublabel, position)."""
        with pytest.raises((TypeError, ValueError)):
            self.strategy.build_sql(self.path_sql, "index_eq", "science", LTree)  # Not a tuple

    def test_index_gte_requires_tuple(self):
        """Test that index_gte operator requires a tuple (sublabel, min_position)."""
        with pytest.raises((TypeError, ValueError)):
            self.strategy.build_sql(self.path_sql, "index_gte", "physics", LTree)  # Not a tuple


class TestLTreePathAnalysisEdgeCases:
    """Test edge cases for path analysis operators."""

    def setup_method(self):
        """Set up test fixtures."""
        self.strategy = LTreeOperatorStrategy()
        self.path_sql = SQL("data->>'path'")

    def test_nlevel_with_zero(self):
        """Test nlevel filtering with zero depth."""
        # Find root level paths (depth = 1)
        result = self.strategy.build_sql(self.path_sql, "nlevel_eq", 1, LTree)
        expected = "nlevel((data->>'path')::ltree) = 1"
        assert result.as_string(None) == expected

    def test_subpath_with_zero_offset_length(self):
        """Test subpath with zero offset and length."""
        # subpath(path, 0, 0) = empty path
        offset, length = 0, 0
        result = self.strategy.build_sql(self.path_sql, "subpath", (offset, length), LTree)
        expected = "subpath((data->>'path')::ltree, 0, 0)"
        assert result.as_string(None) == expected

    def test_index_with_root_label(self):
        """Test index with root-level label."""
        # index('top.science.physics', 'top') = 0
        result = self.strategy.build_sql(self.path_sql, "index", "top", LTree)
        expected = "index((data->>'path')::ltree, 'top'::ltree)"
        assert result.as_string(None) == expected

    def test_index_with_nonexistent_sublabel(self):
        """Test index with sublabel that may not exist."""
        # index('top.science.physics', 'technology') = -1
        result = self.strategy.build_sql(self.path_sql, "index", "technology", LTree)
        expected = "index((data->>'path')::ltree, 'technology'::ltree)"
        assert result.as_string(None) == expected

    def test_filter_by_sublabel_presence(self):
        """Test filtering paths that contain a specific sublabel."""
        # Find paths that contain 'science' anywhere (index >= 0)
        # This would be: index_gte('science', 0)
        sublabel, min_position = "science", 0
        result = self.strategy.build_sql(
            self.path_sql, "index_gte", (sublabel, min_position), LTree
        )
        expected = "index((data->>'path')::ltree, 'science'::ltree) >= 0"
        assert result.as_string(None) == expected
