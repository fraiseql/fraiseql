"""Validation tests for numeric casting consistency.

This addresses the critical issue raised: why cast integers to strings for equality
but to numeric for comparisons? This test validates the corrected behavior.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import get_operator_registry


@pytest.mark.regression
class TestNumericCastingConsistency:
    """Validate that numeric operations are consistent across all operators."""

    def test_numeric_consistency_across_operators(self):
        """All numeric operations should use ::numeric casting consistently."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'port')")
        port_value = 443

        # Test all numeric operations
        numeric_operators = ["eq", "neq", "gt", "gte", "lt", "lte", "in", "notin"]

        for op in numeric_operators:
            strategy = registry.get_strategy(op, int)

            if op in ("in", "notin"):
                # Test with list values
                result = strategy.build_sql(jsonb_path, op, [443, 8080], int)
            else:
                # Test with single value
                result = strategy.build_sql(jsonb_path, op, port_value, int)

            sql_str = str(result)
            print(f"Operator '{op}' SQL: {sql_str}")

            # ALL numeric operations should use ::numeric casting
            assert "::numeric" in sql_str, (
                f"Operator '{op}' should use ::numeric casting for consistency. Got: {sql_str}"
            )

    def test_numeric_comparison_correctness(self):
        """Validate that numeric casting produces correct comparison behavior."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'port')")

        # Test the critical case: numeric ordering
        test_cases = [
            ("gt", 100, "Should find ports > 100"),
            ("gte", 443, "Should find ports >= 443"),
            ("lt", 1000, "Should find ports < 1000"),
            ("lte", 8080, "Should find ports <= 8080"),
        ]

        for op, value, description in test_cases:
            strategy = registry.get_strategy(op, int)
            result = strategy.build_sql(jsonb_path, op, value, int)
            sql_str = str(result)

            print(f"{description}: {sql_str}")

            # Should cast the JSONB field to numeric for proper ordering
            assert "::numeric" in sql_str, f"Numeric comparison {op} needs casting"
            assert f"Literal({value})" in sql_str, f"Should compare with literal {value}"

    def test_boolean_text_consistency(self):
        """Validate that boolean operations use text comparison consistently."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'is_active')")

        # Boolean operations should all use text comparison
        boolean_operators = ["eq", "neq", "in", "notin"]

        for op in boolean_operators:
            strategy = registry.get_strategy(op, bool)

            if op in ("in", "notin"):
                result = strategy.build_sql(jsonb_path, op, [True, False], bool)
            else:
                result = strategy.build_sql(jsonb_path, op, True, bool)

            sql_str = str(result)
            print(f"Boolean operator '{op}' SQL: {sql_str}")

            # Boolean operations should NOT use ::boolean casting
            assert "::boolean" not in sql_str, (
                f"Boolean operator '{op}' should use text comparison, not ::boolean casting. "
                f"Got: {sql_str}"
            )

            # Should convert boolean values to text
            if op in ("eq", "neq"):
                assert "Literal('true')" in sql_str, "Should convert True to 'true'"
            elif op in ("in", "notin"):
                assert "Literal('true')" in sql_str and "Literal('false')" in sql_str, (
                    "Should convert boolean list items to strings"
                )

    def test_mixed_operations_production_scenario(self):
        """Test the realistic scenario that caused the original confusion."""
        registry = get_operator_registry()
        jsonb_port_path = SQL("(data ->> 'port')")
        jsonb_active_path = SQL("(data ->> 'is_active')")

        # Scenario: Find devices where port >= 400 AND is_active = true
        # This should use DIFFERENT casting strategies consistently

        # Port comparison: SHOULD use numeric casting
        port_strategy = registry.get_strategy("gte", int)
        port_result = port_strategy.build_sql(jsonb_port_path, "gte", 400, int)
        port_sql = str(port_result)

        # Boolean equality: SHOULD use text comparison
        bool_strategy = registry.get_strategy("eq", bool)
        bool_result = bool_strategy.build_sql(jsonb_active_path, "eq", True, bool)
        bool_sql = str(bool_result)

        print(f"Port >= 400: {port_sql}")
        print(f"Active = true: {bool_sql}")

        # Validate the different but consistent approaches
        assert "::numeric" in port_sql, "Port comparison needs numeric casting"
        assert "::boolean" not in bool_sql, "Boolean comparison should use text"
        assert "Literal('true')" in bool_sql, "Boolean should be converted to text"

        # This combination would produce valid SQL:
        # WHERE (data->>'port')::numeric >= 400 AND data->>'is_active' = 'true'


@pytest.mark.regression
class TestCastingEdgeCases:
    """Test edge cases that could break the casting logic."""

    def test_boolean_subclass_of_int_handled(self):
        """Ensure bool values don't get numeric casting (bool is subclass of int)."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'flag')")

        # This is the critical test: isinstance(True, int) returns True in Python!
        assert isinstance(True, int), "Sanity check: bool is subclass of int in Python"

        strategy = registry.get_strategy("eq", bool)
        result = strategy.build_sql(jsonb_path, "eq", True, bool)
        sql_str = str(result)

        print(f"Boolean handling: {sql_str}")

        # Should NOT get numeric casting despite bool being subclass of int
        assert "::numeric" not in sql_str, "Bool should not get numeric casting"
        assert "::boolean" not in sql_str, "Bool should not get boolean casting"
        assert "Literal('true')" in sql_str, "Bool should convert to text"

    def test_numeric_list_operations(self):
        """Test that list operations maintain numeric casting consistency."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'port')")

        # Test IN operation with numeric list
        strategy = registry.get_strategy("in", int)
        result = strategy.build_sql(jsonb_path, "in", [80, 443, 8080], int)
        sql_str = str(result)

        print(f"Port IN list: {sql_str}")

        # Should use numeric casting for the field
        assert "::numeric" in sql_str, "List operations should use numeric casting"
        # Values should remain as integers (individual literals, not array)
        assert (
            "Literal(80)" in sql_str and "Literal(443)" in sql_str and "Literal(8080)" in sql_str
        ), "Integer values should be individual literals"


if __name__ == "__main__":
    print("Testing numeric casting consistency...")
    print(
        "Run with: pytest tests/regression/where_clause/test_numeric_consistency_validation.py -v -s"
    )
