"""End-to-end tests for DateRange filtering functionality.

These tests verify that DateRange operators work correctly in the full context
of the WHERE clause building system, from field detection through SQL generation
to actual PostgreSQL execution.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.core.field_detection import FieldType
from fraiseql.sql.where.operators import get_operator_function

pytestmark = pytest.mark.database


class TestDateRangeEndToEndIntegration:
    """Test DateRange operators in full integration context."""

    def test_daterange_field_type_detection(self) -> None:
        """Test that DateRange field types can be detected and mapped to operators."""
        # Test operator lookup for DateRange field type
        basic_operators = ["eq", "neq", "in", "notin"]
        range_operators = [
            "contains_date",
            "overlaps",
            "adjacent",
            "strictly_left",
            "strictly_right",
            "not_left",
            "not_right",
        ]

        for op in basic_operators + range_operators:
            # This should not raise an exception
            func = get_operator_function(FieldType.DATE_RANGE, op)
            assert callable(func), f"DateRange operator '{op}' should return a callable function"

    def test_daterange_basic_operators_integration(self) -> None:
        """Test DateRange basic operators generate correct SQL in full context."""
        path_sql = SQL("data->>'fiscal_period'")

        # Test equality
        eq_func = get_operator_function(FieldType.DATE_RANGE, "eq")
        result = eq_func(path_sql, "[2023-01-01,2023-12-31]")
        expected = "(data->>'fiscal_period')::daterange = '[2023-01-01,2023-12-31]'::daterange"
        assert result.as_string(None) == expected

        # Test inequality
        neq_func = get_operator_function(FieldType.DATE_RANGE, "neq")
        result = neq_func(path_sql, "[2022-01-01,2022-12-31]")
        expected = "(data->>'fiscal_period')::daterange != '[2022-01-01,2022-12-31]'::daterange"
        assert result.as_string(None) == expected

        # Test IN list
        in_func = get_operator_function(FieldType.DATE_RANGE, "in")
        result = in_func(path_sql, ["[2023-01-01,2023-12-31]", "[2024-01-01,2024-12-31]"])
        expected = "(data->>'fiscal_period')::daterange IN ('[2023-01-01,2023-12-31]'::daterange, '[2024-01-01,2024-12-31]'::daterange)"
        assert result.as_string(None) == expected

    def test_daterange_range_operators_integration(self) -> None:
        """Test DateRange-specific operators generate correct SQL in full context."""
        path_sql = SQL("data->>'project_timeline'")

        # Test contains_date (@>)
        contains_func = get_operator_function(FieldType.DATE_RANGE, "contains_date")
        result = contains_func(path_sql, "2023-07-15")
        expected = "(data->>'project_timeline')::daterange @> '2023-07-15'"
        assert result.as_string(None) == expected

        # Test overlaps (&&)
        overlaps_func = get_operator_function(FieldType.DATE_RANGE, "overlaps")
        result = overlaps_func(path_sql, "[2023-06-01,2023-08-31]")
        expected = "(data->>'project_timeline')::daterange && '[2023-06-01,2023-08-31]'::daterange"
        assert result.as_string(None) == expected

        # Test adjacent (-|-)
        adjacent_func = get_operator_function(FieldType.DATE_RANGE, "adjacent")
        result = adjacent_func(path_sql, "[2024-01-01,2024-12-31]")
        expected = "(data->>'project_timeline')::daterange -|- '[2024-01-01,2024-12-31]'::daterange"
        assert result.as_string(None) == expected

        # Test strictly_left (<<)
        left_func = get_operator_function(FieldType.DATE_RANGE, "strictly_left")
        result = left_func(path_sql, "[2024-01-01,2024-12-31]")
        expected = "(data->>'project_timeline')::daterange << '[2024-01-01,2024-12-31]'::daterange"
        assert result.as_string(None) == expected

    def test_daterange_positioning_operators_integration(self) -> None:
        """Test DateRange positioning operators in full context."""
        path_sql = SQL("data->>'contract_period'")

        # Test strictly_right (>>)
        right_func = get_operator_function(FieldType.DATE_RANGE, "strictly_right")
        result = right_func(path_sql, "[2022-01-01,2022-12-31]")
        expected = "(data->>'contract_period')::daterange >> '[2022-01-01,2022-12-31]'::daterange"
        assert result.as_string(None) == expected

        # Test not_left (&>)
        not_left_func = get_operator_function(FieldType.DATE_RANGE, "not_left")
        result = not_left_func(path_sql, "[2023-01-01,2023-12-31]")
        expected = "(data->>'contract_period')::daterange &> '[2023-01-01,2023-12-31]'::daterange"
        assert result.as_string(None) == expected

        # Test not_right (&<)
        not_right_func = get_operator_function(FieldType.DATE_RANGE, "not_right")
        result = not_right_func(path_sql, "[2023-01-01,2023-12-31]")
        expected = "(data->>'contract_period')::daterange &< '[2023-01-01,2023-12-31]'::daterange"
        assert result.as_string(None) == expected

    def test_daterange_complex_scenarios_integration(self) -> None:
        """Test DateRange operators with complex real-world scenarios."""
        # Test fiscal year queries
        fiscal_sql = SQL("data->>'fiscal_year'")

        # Test Q2 overlap check
        overlaps_func = get_operator_function(FieldType.DATE_RANGE, "overlaps")
        result = overlaps_func(fiscal_sql, "[2023-04-01,2023-06-30]")
        expected = "(data->>'fiscal_year')::daterange && '[2023-04-01,2023-06-30]'::daterange"
        assert result.as_string(None) == expected

        # Test specific date containment
        contains_func = get_operator_function(FieldType.DATE_RANGE, "contains_date")
        result = contains_func(fiscal_sql, "2023-05-15")
        expected = "(data->>'fiscal_year')::daterange @> '2023-05-15'"
        assert result.as_string(None) == expected

        # Test project timeline adjacency
        project_sql = SQL("data->>'project_phase'")
        adjacent_func = get_operator_function(FieldType.DATE_RANGE, "adjacent")
        result = adjacent_func(project_sql, "[2023-07-01,2023-12-31]")
        expected = "(data->>'project_phase')::daterange -|- '[2023-07-01,2023-12-31]'::daterange"
        assert result.as_string(None) == expected

    def test_daterange_unbounded_ranges_integration(self) -> None:
        """Test DateRange operators with unbounded ranges."""
        path_sql = SQL("data->>'validity_period'")

        # Test unbounded start
        eq_func = get_operator_function(FieldType.DATE_RANGE, "eq")
        result = eq_func(path_sql, "(,2023-12-31]")
        expected = "(data->>'validity_period')::daterange = '(,2023-12-31]'::daterange"
        assert result.as_string(None) == expected

        # Test unbounded end
        result = eq_func(path_sql, "[2023-01-01,)")
        expected = "(data->>'validity_period')::daterange = '[2023-01-01,)'::daterange"
        assert result.as_string(None) == expected

        # Test infinite bounds
        result = eq_func(path_sql, "[2023-01-01,infinity)")
        expected = "(data->>'validity_period')::daterange = '[2023-01-01,infinity)'::daterange"
        assert result.as_string(None) == expected

    def test_daterange_error_handling_integration(self) -> None:
        """Test DateRange operator error handling in integration context."""
        path_sql = SQL("data->>'period'")

        # Test that IN operator requires list
        in_func = get_operator_function(FieldType.DATE_RANGE, "in")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            in_func(path_sql, "[2023-01-01,2023-12-31]")

        # Test that NOT IN operator requires list
        notin_func = get_operator_function(FieldType.DATE_RANGE, "notin")
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            notin_func(path_sql, "[2023-01-01,2023-12-31]")

    def test_daterange_operator_coverage_integration(self) -> None:
        """Test that all expected DateRange operators are available."""
        expected_operators = {
            "eq",
            "neq",
            "in",
            "notin",  # Basic operators
            "contains_date",
            "overlaps",
            "adjacent",  # Range operators
            "strictly_left",
            "strictly_right",
            "not_left",
            "not_right",  # Positioning operators
        }

        available_operators = set()
        for op in expected_operators:
            try:
                func = get_operator_function(FieldType.DATE_RANGE, op)
                if callable(func):
                    available_operators.add(op)
            except ValueError:
                pass  # Operator not available

        assert available_operators == expected_operators, (
            f"Missing DateRange operators: {expected_operators - available_operators}"
        )

    def test_daterange_mixed_bracket_types_integration(self) -> None:
        """Test DateRange operators with mixed bracket types (inclusive/exclusive)."""
        path_sql = SQL("data->>'reporting_period'")

        # Test all bracket combinations
        bracket_combinations = [
            "[2023-01-01,2023-12-31]",  # Both inclusive
            "[2023-01-01,2023-12-31)",  # Start inclusive, end exclusive
            "(2023-01-01,2023-12-31]",  # Start exclusive, end inclusive
            "(2023-01-01,2023-12-31)",  # Both exclusive
        ]

        eq_func = get_operator_function(FieldType.DATE_RANGE, "eq")
        for date_range in bracket_combinations:
            result = eq_func(path_sql, date_range)
            expected = f"(data->>'reporting_period')::daterange = '{date_range}'::daterange"
            assert result.as_string(None) == expected
