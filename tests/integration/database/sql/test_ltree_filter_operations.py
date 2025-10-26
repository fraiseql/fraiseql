"""Integration tests for LTree hierarchical path filtering operations.

Tests the SQL generation and database execution of LTree filters
to ensure proper PostgreSQL ltree type handling with hierarchical operators.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import get_operator_registry
from fraiseql.types import LTree


@pytest.mark.integration
class TestLTreeFilterOperations:
    """Test LTree hierarchical filtering with proper PostgreSQL ltree operators."""

    def test_ltree_ancestor_of_operation(self):
        """Test LTree ancestor_of operation (@>)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="ancestor_of",
            val="departments.engineering.backend",
            field_type=LTree,
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "@>" in sql_str, "Missing ancestor operator"
        assert "departments.engineering.backend" in sql_str

    def test_ltree_descendant_of_operation(self):
        """Test LTree descendant_of operation (<@)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        sql = registry.build_sql(
            path_sql=path_sql, op="descendant_of", val="departments.engineering", field_type=LTree
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "<@" in sql_str, "Missing descendant operator"
        assert "departments.engineering" in sql_str

    def test_ltree_matches_lquery_operation(self):
        """Test LTree matches_lquery operation (~)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # Test with wildcard pattern
        sql = registry.build_sql(
            path_sql=path_sql, op="matches_lquery", val="*.engineering.*", field_type=LTree
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "~" in sql_str, "Missing lquery match operator"
        assert "*.engineering.*" in sql_str

    def test_ltree_matches_ltxtquery_operation(self):
        """Test LTree matches_ltxtquery operation (?)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # Test with text query
        sql = registry.build_sql(
            path_sql=path_sql, op="matches_ltxtquery", val="engineering & backend", field_type=LTree
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "?" in sql_str, "Missing ltxtquery match operator"
        assert "engineering & backend" in sql_str

    def test_ltree_eq_operation_with_casting(self):
        """Test that basic equality uses ltree casting for consistency."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        sql = registry.build_sql(
            path_sql=path_sql, op="eq", val="departments.engineering.backend", field_type=LTree
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "=" in sql_str, "Missing equality operator"
        assert "departments.engineering.backend" in sql_str

    def test_ltree_neq_operation_with_casting(self):
        """Test that inequality uses ltree casting for consistency."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        sql = registry.build_sql(
            path_sql=path_sql, op="neq", val="departments.marketing", field_type=LTree
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "!=" in sql_str, "Missing inequality operator"
        assert "departments.marketing" in sql_str

    def test_ltree_isnull_operation(self):
        """Test LTree NULL check operations."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # Test IS NULL
        sql_null = registry.build_sql(path_sql=path_sql, op="isnull", val=True, field_type=LTree)
        assert "IS NULL" in str(sql_null)

        # Test IS NOT NULL
        sql_not_null = registry.build_sql(
            path_sql=path_sql, op="isnull", val=False, field_type=LTree
        )
        assert "IS NOT NULL" in str(sql_not_null)

    def test_ltree_in_list_with_casting(self):
        """Test LTree IN operation with proper ltree casting."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        paths = ["departments.engineering", "departments.marketing", "departments.sales"]

        sql = registry.build_sql(path_sql=path_sql, op="in", val=paths, field_type=LTree)

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "IN" in sql_str, "Missing IN operator"
        for path in paths:
            assert path in sql_str

    def test_ltree_nin_operation_with_casting(self):
        """Test LTree NOT IN operation with proper ltree casting."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        excluded_paths = ["departments.hr", "departments.legal"]

        sql = registry.build_sql(
            path_sql=path_sql, op="notin", val=excluded_paths, field_type=LTree
        )

        sql_str = str(sql)
        assert "::ltree" in sql_str, "Missing ltree cast"
        assert "NOT IN" in sql_str, "Missing NOT IN operator"
        for path in excluded_paths:
            assert path in sql_str

    def test_ltree_filter_excludes_pattern_operators(self):
        """Test that LTree doesn't allow generic pattern operators."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # These generic pattern operators should not be available for LTree
        problematic_ops = ["contains", "startswith", "endswith"]

        for op in problematic_ops:
            with pytest.raises(
                ValueError, match=f"Pattern operator '{op}' is not supported for LTree fields"
            ):
                registry.build_sql(path_sql=path_sql, op=op, val="engineering", field_type=LTree)

    def test_ltree_vs_string_field_behavior(self):
        """Test that LTree fields get different treatment than string fields."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'some_field'")

        # For LTree fields, should use ltree casting
        ltree_sql = registry.build_sql(
            path_sql=path_sql, op="eq", val="departments.engineering", field_type=LTree
        )
        ltree_sql_str = str(ltree_sql)
        assert "::ltree" in ltree_sql_str

        # For regular string fields, should NOT use ltree casting
        string_sql = registry.build_sql(
            path_sql=path_sql, op="eq", val="departments.engineering", field_type=str
        )
        string_sql_str = str(string_sql)
        assert "::ltree" not in string_sql_str

    def test_ltree_hierarchical_relationships(self):
        """Test typical hierarchical relationship scenarios."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # Test cases for typical hierarchical queries
        test_cases = [
            {
                "description": "Find all descendants of engineering dept",
                "op": "descendant_of",
                "val": "departments.engineering",
                "expected_op": "<@",
            },
            {
                "description": "Find all ancestors of backend team",
                "op": "ancestor_of",
                "val": "departments.engineering.backend.api",
                "expected_op": "@>",
            },
            {
                "description": "Find paths matching engineering pattern",
                "op": "matches_lquery",
                "val": "*.engineering.*",
                "expected_op": "~",
            },
        ]

        for case in test_cases:
            sql = registry.build_sql(
                path_sql=path_sql, op=case["op"], val=case["val"], field_type=LTree
            )

            sql_str = str(sql)
            assert "::ltree" in sql_str, f"Missing ltree cast for {case['description']}"
            assert case["expected_op"] in sql_str, (
                f"Missing {case['expected_op']} for {case['description']}"
            )
            assert case["val"] in sql_str, f"Missing value for {case['description']}"

    def test_ltree_advanced_lquery_patterns(self):
        """Test advanced lquery pattern matching.

        This test should pass once LTreeOperatorStrategy is implemented.
        It verifies complex lquery patterns work correctly.
        """
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # Complex lquery patterns
        advanced_patterns = [
            "*.{engineering,marketing}.*",  # Match either engineering or marketing
            "departments.*{2,3}",  # Match 2-3 levels under departments
            "*.!sales.*",  # Match anything except sales
        ]

        for pattern in advanced_patterns:
            sql = registry.build_sql(
                path_sql=path_sql, op="matches_lquery", val=pattern, field_type=LTree
            )

            sql_str = str(sql)
            assert "::ltree" in sql_str
            assert "~" in sql_str
            assert pattern in sql_str

    def test_ltree_ltxtquery_boolean_operations(self):
        """Test ltxtquery boolean operations.

        This test should pass once LTreeOperatorStrategy is implemented.
        It verifies ltxtquery boolean logic works correctly.
        """
        registry = get_operator_registry()
        path_sql = SQL("data->>'path'")

        # Boolean ltxtquery patterns
        boolean_queries = [
            "engineering & backend",  # AND operation
            "marketing | sales",  # OR operation
            "engineering & !frontend",  # AND NOT operation
        ]

        for query in boolean_queries:
            sql = registry.build_sql(
                path_sql=path_sql, op="matches_ltxtquery", val=query, field_type=LTree
            )

            sql_str = str(sql)
            assert "::ltree" in sql_str
            assert "?" in sql_str
            assert query in sql_str
