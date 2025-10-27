"""Integration tests for DateRange filtering operations.

Tests the SQL generation and database execution of DateRange filters
to ensure proper PostgreSQL daterange type handling with range operators.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import get_operator_registry
from fraiseql.types.scalars.daterange import DateRangeField


@pytest.mark.integration
class TestDateRangeFilterOperations:
    """Test DateRange filtering with proper PostgreSQL daterange operators."""

    def test_daterange_contains_date_operation(self):
        """Test DateRange contains_date operation (@>)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql, op="contains_date", val="2023-06-15", field_type=DateRangeField
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "@>" in sql_str, "Missing contains operator"
        assert "2023-06-15" in sql_str

    def test_daterange_overlaps_operation(self):
        """Test DateRange overlaps operation (&&)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="overlaps",
            val="[2023-06-01,2023-06-30]",
            field_type=DateRangeField,
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "&&" in sql_str, "Missing overlaps operator"
        assert "[2023-06-01,2023-06-30]" in sql_str

    def test_daterange_adjacent_operation(self):
        """Test DateRange adjacent operation (-|-)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="adjacent",
            val="[2023-07-01,2023-07-31]",
            field_type=DateRangeField,
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "-|-" in sql_str, "Missing adjacent operator"
        assert "[2023-07-01,2023-07-31]" in sql_str

    def test_daterange_strictly_left_operation(self):
        """Test DateRange strictly_left operation (<<)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="strictly_left",
            val="[2023-07-01,2023-12-31]",
            field_type=DateRangeField,
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "<<" in sql_str, "Missing strictly left operator"
        assert "[2023-07-01,2023-12-31]" in sql_str

    def test_daterange_strictly_right_operation(self):
        """Test DateRange strictly_right operation (>>)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="strictly_right",
            val="[2023-01-01,2023-06-30]",
            field_type=DateRangeField,
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert ">>" in sql_str, "Missing strictly right operator"
        assert "[2023-01-01,2023-06-30]" in sql_str

    def test_daterange_not_left_operation(self):
        """Test DateRange not_left operation (&>)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="not_left",
            val="[2023-01-01,2023-06-30]",
            field_type=DateRangeField,
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "&>" in sql_str, "Missing not left operator"
        assert "[2023-01-01,2023-06-30]" in sql_str

    def test_daterange_not_right_operation(self):
        """Test DateRange not_right operation (&<)."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql,
            op="not_right",
            val="[2023-07-01,2023-12-31]",
            field_type=DateRangeField,
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "&<" in sql_str, "Missing not right operator"
        assert "[2023-07-01,2023-12-31]" in sql_str

    def test_daterange_eq_operation_with_casting(self):
        """Test that basic equality uses daterange casting for consistency."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql, op="eq", val="[2023-01-01,2023-12-31]", field_type=DateRangeField
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "=" in sql_str, "Missing equality operator"
        assert "[2023-01-01,2023-12-31]" in sql_str

    def test_daterange_neq_operation_with_casting(self):
        """Test that inequality uses daterange casting for consistency."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        sql = registry.build_sql(
            path_sql=path_sql, op="neq", val="[2023-01-01,2023-06-30]", field_type=DateRangeField
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "!=" in sql_str, "Missing inequality operator"
        assert "[2023-01-01,2023-06-30]" in sql_str

    def test_daterange_isnull_operation(self):
        """Test DateRange NULL check operations."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        # Test IS NULL
        sql_null = registry.build_sql(
            path_sql=path_sql, op="isnull", val=True, field_type=DateRangeField
        )
        assert "IS NULL" in str(sql_null)

        # Test IS NOT NULL
        sql_not_null = registry.build_sql(
            path_sql=path_sql, op="isnull", val=False, field_type=DateRangeField
        )
        assert "IS NOT NULL" in str(sql_not_null)

    def test_daterange_in_list_with_casting(self):
        """Test DateRange IN operation with proper daterange casting."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        ranges = [
            "[2023-01-01,2023-03-31]",  # Q1
            "[2023-04-01,2023-06-30]",  # Q2
            "[2023-07-01,2023-09-30]",  # Q3
        ]

        sql = registry.build_sql(path_sql=path_sql, op="in", val=ranges, field_type=DateRangeField)

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "IN" in sql_str, "Missing IN operator"
        for range_val in ranges:
            assert range_val in sql_str

    def test_daterange_nin_operation_with_casting(self):
        """Test DateRange NOT IN operation with proper daterange casting."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        excluded_ranges = [
            "[2023-12-01,2023-12-31]",  # December
            "[2023-01-01,2023-01-31]",  # January
        ]

        sql = registry.build_sql(
            path_sql=path_sql, op="notin", val=excluded_ranges, field_type=DateRangeField
        )

        sql_str = str(sql)
        assert "::daterange" in sql_str, "Missing daterange cast"
        assert "NOT IN" in sql_str, "Missing NOT IN operator"
        for range_val in excluded_ranges:
            assert range_val in sql_str

    def test_daterange_filter_excludes_pattern_operators(self):
        """Test that DateRange doesn't allow generic pattern operators."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        # These generic pattern operators should not be available for DateRange
        problematic_ops = ["contains", "startswith", "endswith"]

        for op in problematic_ops:
            with pytest.raises(
                ValueError, match=f"Pattern operator '{op}' is not supported for DateRange fields"
            ):
                registry.build_sql(path_sql=path_sql, op=op, val="2023", field_type=DateRangeField)

    def test_daterange_vs_string_field_behavior(self):
        """Test that DateRange fields get different treatment than string fields."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'some_field'")

        # For DateRange fields, should use daterange casting
        daterange_sql = registry.build_sql(
            path_sql=path_sql, op="eq", val="[2023-01-01,2023-12-31]", field_type=DateRangeField
        )
        daterange_sql_str = str(daterange_sql)
        assert "::daterange" in daterange_sql_str

        # For regular string fields, should NOT use daterange casting
        string_sql = registry.build_sql(
            path_sql=path_sql, op="eq", val="[2023-01-01,2023-12-31]", field_type=str
        )
        string_sql_str = str(string_sql)
        assert "::daterange" not in string_sql_str

    def test_daterange_typical_use_cases(self):
        """Test typical date range query scenarios."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        # Test cases for typical range queries
        test_cases = [
            {
                "description": "Check if event period contains specific date",
                "op": "contains_date",
                "val": "2023-06-15",
                "expected_op": "@>",
            },
            {
                "description": "Find overlapping periods",
                "op": "overlaps",
                "val": "[2023-06-01,2023-06-30]",
                "expected_op": "&&",
            },
            {
                "description": "Find adjacent periods",
                "op": "adjacent",
                "val": "[2023-07-01,2023-07-31]",
                "expected_op": "-|-",
            },
            {
                "description": "Find periods strictly before another",
                "op": "strictly_left",
                "val": "[2023-07-01,2023-12-31]",
                "expected_op": "<<",
            },
        ]

        for case in test_cases:
            sql = registry.build_sql(
                path_sql=path_sql, op=case["op"], val=case["val"], field_type=DateRangeField
            )

            sql_str = str(sql)
            assert "::daterange" in sql_str, f"Missing daterange cast for {case['description']}"
            assert case["expected_op"] in sql_str, (
                f"Missing {case['expected_op']} for {case['description']}"
            )
            assert case["val"] in sql_str, f"Missing value for {case['description']}"

    def test_daterange_complex_range_queries(self):
        """Test complex range query combinations.

        This test should pass once DateRangeOperatorStrategy is implemented.
        It verifies complex range operations work correctly.
        """
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        # Complex range scenarios
        complex_scenarios = [
            {
                "op": "overlaps",
                "val": "[2023-01-15,2023-02-15]",  # Overlaps month boundary
                "description": "Cross-month overlap",
            },
            {
                "op": "contains_date",
                "val": "2023-12-25",  # Christmas
                "description": "Holiday date containment",
            },
            {
                "op": "adjacent",
                "val": "(2023-06-30,2023-08-01)",  # Adjacent to July
                "description": "Adjacent exclusive ranges",
            },
        ]

        for scenario in complex_scenarios:
            sql = registry.build_sql(
                path_sql=path_sql, op=scenario["op"], val=scenario["val"], field_type=DateRangeField
            )

            sql_str = str(sql)
            assert "::daterange" in sql_str
            assert scenario["val"] in sql_str

    def test_daterange_inclusive_exclusive_boundaries(self):
        """Test inclusive vs exclusive range boundaries.

        This test should pass once DateRangeOperatorStrategy is implemented.
        It verifies boundary handling works correctly.
        """
        registry = get_operator_registry()
        path_sql = SQL("data->>'period'")

        # Different boundary types
        boundary_types = [
            "[2023-01-01,2023-12-31]",  # Inclusive both ends
            "[2023-01-01,2023-12-31)",  # Inclusive start, exclusive end
            "(2023-01-01,2023-12-31]",  # Exclusive start, inclusive end
            "(2023-01-01,2023-12-31)",  # Exclusive both ends
        ]

        for boundary in boundary_types:
            sql = registry.build_sql(
                path_sql=path_sql, op="eq", val=boundary, field_type=DateRangeField
            )

            sql_str = str(sql)
            assert "::daterange" in sql_str
            assert boundary in sql_str
