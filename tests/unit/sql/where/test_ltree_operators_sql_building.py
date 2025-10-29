#!/usr/bin/env python3
"""Test LTree hierarchical path operators SQL building.

This module tests the clean LTree operator functions that generate
proper PostgreSQL SQL with ::ltree casting and specialized hierarchical operators.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators import ltree


class TestLTreeBasicOperators:
    """Test LTree basic operators SQL building with proper PostgreSQL ltree casting."""

    def test_build_ltree_equality_sql(self):
        """Test LTree equality SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltree_value = "top.science.astrophysics"

        result = ltree.build_ltree_eq_sql(path_sql, ltree_value)

        expected_sql = "(data->>'category_path')::ltree = 'top.science.astrophysics'::ltree"
        assert result.as_string(None) == expected_sql

    def test_build_ltree_inequality_sql(self):
        """Test LTree inequality SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltree_value = "top.technology.computing"

        result = ltree.build_ltree_neq_sql(path_sql, ltree_value)

        expected_sql = "(data->>'category_path')::ltree != 'top.technology.computing'::ltree"
        assert result.as_string(None) == expected_sql

    def test_build_ltree_in_list_sql(self):
        """Test LTree IN list SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltree_list = ["top.science.physics", "top.science.chemistry", "top.technology.computing"]

        result = ltree.build_ltree_in_sql(path_sql, ltree_list)

        expected_sql = "(data->>'category_path')::ltree IN ('top.science.physics'::ltree, 'top.science.chemistry'::ltree, 'top.technology.computing'::ltree)"
        assert result.as_string(None) == expected_sql

    def test_build_ltree_not_in_list_sql(self):
        """Test LTree NOT IN list SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltree_list = ["top.science.physics", "top.science.chemistry"]

        result = ltree.build_ltree_notin_sql(path_sql, ltree_list)

        expected_sql = "(data->>'category_path')::ltree NOT IN ('top.science.physics'::ltree, 'top.science.chemistry'::ltree)"
        assert result.as_string(None) == expected_sql

    def test_build_ltree_single_item_in_list(self):
        """Test LTree IN with single item."""
        path_sql = SQL("data->>'category_path'")
        ltree_list = ["top.science.astrophysics"]

        result = ltree.build_ltree_in_sql(path_sql, ltree_list)

        expected_sql = "(data->>'category_path')::ltree IN ('top.science.astrophysics'::ltree)"
        assert result.as_string(None) == expected_sql

    def test_build_ltree_empty_list_handling(self):
        """Test LTree operators with empty lists."""
        path_sql = SQL("data->>'category_path'")
        empty_list = []

        # Empty IN list should generate valid SQL
        result_in = ltree.build_ltree_in_sql(path_sql, empty_list)
        expected_in = "(data->>'category_path')::ltree IN ()"
        assert result_in.as_string(None) == expected_in

        # Empty NOT IN list should generate valid SQL
        result_notin = ltree.build_ltree_notin_sql(path_sql, empty_list)
        expected_notin = "(data->>'category_path')::ltree NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestLTreeHierarchicalOperators:
    """Test LTree hierarchical operators with PostgreSQL ltree-specific syntax."""

    def test_build_ancestor_of_sql(self):
        """Test LTree ancestor_of (@>) SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltree_value = "top.science"

        result = ltree.build_ancestor_of_sql(path_sql, ltree_value)

        expected_sql = "(data->>'category_path')::ltree @> 'top.science'::ltree"
        assert result.as_string(None) == expected_sql

    def test_build_descendant_of_sql(self):
        """Test LTree descendant_of (<@) SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltree_value = "top.science.astrophysics.black_holes"

        result = ltree.build_descendant_of_sql(path_sql, ltree_value)

        expected_sql = (
            "(data->>'category_path')::ltree <@ 'top.science.astrophysics.black_holes'::ltree"
        )
        assert result.as_string(None) == expected_sql

    def test_build_matches_lquery_sql(self):
        """Test LTree matches_lquery (~) SQL generation."""
        path_sql = SQL("data->>'category_path'")
        lquery_pattern = "science.*"

        result = ltree.build_matches_lquery_sql(path_sql, lquery_pattern)

        expected_sql = "(data->>'category_path')::ltree ~ 'science.*'::lquery"
        assert result.as_string(None) == expected_sql

    def test_build_matches_ltxtquery_sql(self):
        """Test LTree matches_ltxtquery (?) SQL generation."""
        path_sql = SQL("data->>'category_path'")
        ltxtquery_pattern = "astrophysics"

        result = ltree.build_matches_ltxtquery_sql(path_sql, ltxtquery_pattern)

        expected_sql = "(data->>'category_path')::ltree ? 'astrophysics'::ltxtquery"
        assert result.as_string(None) == expected_sql

    def test_hierarchical_operators_complex_paths(self):
        """Test hierarchical operators with complex nested paths."""
        path_sql = SQL("data->>'navigation_path'")

        # Test deeply nested ancestor relationship
        deep_ancestor = "top.academics.university.department.faculty.professor.research"
        result_ancestor = ltree.build_ancestor_of_sql(path_sql, deep_ancestor)
        expected_ancestor = "(data->>'navigation_path')::ltree @> 'top.academics.university.department.faculty.professor.research'::ltree"
        assert result_ancestor.as_string(None) == expected_ancestor

        # Test complex lquery pattern
        complex_pattern = "top.academics.university.*.faculty.*"
        result_lquery = ltree.build_matches_lquery_sql(path_sql, complex_pattern)
        expected_lquery = (
            "(data->>'navigation_path')::ltree ~ 'top.academics.university.*.faculty.*'::lquery"
        )
        assert result_lquery.as_string(None) == expected_lquery


class TestLTreeValidation:
    """Test LTree validation and error handling."""

    def test_ltree_in_requires_list(self):
        """Test that LTree IN operator requires a list."""
        path_sql = SQL("data->>'category_path'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            ltree.build_ltree_in_sql(path_sql, "not-a-list")

    def test_ltree_notin_requires_list(self):
        """Test that LTree NOT IN operator requires a list."""
        path_sql = SQL("data->>'category_path'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            ltree.build_ltree_notin_sql(path_sql, "not-a-list")

    def test_ltree_path_formats(self):
        """Test LTree operators with various path formats."""
        path_sql = SQL("data->>'category_path'")

        # Test with underscores
        path_with_underscores = "top.tech_category.web_development"
        result = ltree.build_ltree_eq_sql(path_sql, path_with_underscores)
        expected = "(data->>'category_path')::ltree = 'top.tech_category.web_development'::ltree"
        assert result.as_string(None) == expected

        # Test with numbers
        path_with_numbers = "top.version_2.release_1"
        result_numbers = ltree.build_ltree_eq_sql(path_sql, path_with_numbers)
        expected_numbers = "(data->>'category_path')::ltree = 'top.version_2.release_1'::ltree"
        assert result_numbers.as_string(None) == expected_numbers

    def test_ltree_single_level_paths(self):
        """Test LTree operators with single-level paths."""
        path_sql = SQL("data->>'category_path'")
        single_level = "root"

        # Even single level should work with ltree casting
        result = ltree.build_ltree_eq_sql(path_sql, single_level)
        expected = "(data->>'category_path')::ltree = 'root'::ltree"
        assert result.as_string(None) == expected
