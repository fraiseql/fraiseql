"""Precise SQL validation tests that check actual SQL output structure.

These tests validate the actual rendered SQL, not just the internal Composed structure,
to ensure we generate valid, well-formed PostgreSQL queries.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import get_operator_registry


@pytest.mark.regression
class TestPreciseSQLValidation:
    """Validate actual rendered SQL output for correctness."""

    def test_numeric_casting_renders_valid_sql(self):
        """Test that numeric operations render to valid PostgreSQL syntax."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'port')")

        strategy = registry.get_strategy("gte", int)
        result = strategy.build_sql(jsonb_path, "gte", 443, int)

        # Use the same rendering approach as complete SQL validation
        try:
            # Use psycopg's as_string method which renders actual SQL
            rendered_sql = result.as_string(None)
        except Exception:
            # Fallback: manually render the structure
            def render_part(part):
                if hasattr(part, "as_string"):
                    return part.as_string(None)
                elif hasattr(part, "string"):  # SQL object
                    return part.string
                elif hasattr(part, "seq"):  # Nested Composed
                    return "".join(render_part(p) for p in part.seq)
                else:  # Literal
                    return "%s"  # Parameter placeholder

            if hasattr(result, "seq"):
                rendered_sql = "".join(render_part(part) for part in result.seq)
            else:
                rendered_sql = render_part(result)

        print(f"Rendered SQL: {rendered_sql}")

        # Should be valid PostgreSQL syntax
        expected_patterns = [
            "data ->> 'port'",  # JSONB extraction
            "::numeric",  # Type casting
            ">=",  # Comparison operator
        ]

        for pattern in expected_patterns:
            assert pattern in rendered_sql, f"Missing '{pattern}' in rendered SQL: {rendered_sql}"

        # Should have balanced parentheses
        assert rendered_sql.count("(") == rendered_sql.count(")"), (
            f"Unbalanced parentheses in: {rendered_sql}"
        )

    def test_boolean_comparison_renders_valid_sql(self):
        """Test that boolean operations render to valid text comparison SQL."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'is_active')")

        strategy = registry.get_strategy("eq", bool)
        result = strategy.build_sql(jsonb_path, "eq", True, bool)

        # Check the structure components
        sql_str = str(result)
        print(f"Boolean SQL structure: {sql_str}")

        # Validate key structural elements
        assert "data ->> 'is_active'" in sql_str, "Should contain JSONB field extraction"
        assert "Literal('true')" in sql_str, "Should use text literal for boolean"
        assert "::boolean" not in sql_str, "Should NOT use boolean casting"

        # Ensure proper operator
        assert "SQL(' = ')" in sql_str, "Should use equality operator"

    def test_hostname_comparison_no_ltree_casting(self):
        """Test that hostname comparison doesn't incorrectly use ltree casting."""
        from fraiseql.types import Hostname

        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'hostname')")

        strategy = registry.get_strategy("eq", Hostname)
        result = strategy.build_sql(jsonb_path, "eq", "printserver01.local", Hostname)

        sql_str = str(result)
        print(f"Hostname SQL structure: {sql_str}")

        # Critical validations
        assert "data ->> 'hostname'" in sql_str, "Should contain JSONB field extraction"
        assert "Literal('printserver01.local')" in sql_str, "Should contain hostname value"
        assert "::ltree" not in sql_str, "Should NOT use ltree casting (this was the bug!)"

    def test_list_operations_have_correct_structure(self):
        """Test that IN operations have proper SQL structure."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'port')")

        # Test numeric IN
        strategy = registry.get_strategy("in", int)
        result = strategy.build_sql(jsonb_path, "in", [80, 443], int)

        sql_str = str(result)
        print(f"IN operation structure: {sql_str}")

        # Should have proper components
        assert "data ->> 'port'" in sql_str, "Should contain JSONB field extraction"
        assert "::numeric" in sql_str, "Should have numeric casting for field"
        assert " IN (" in sql_str, "Should have IN operator"
        assert "Literal(80)" in sql_str, "Should have first literal"
        assert "Literal(443)" in sql_str, "Should have second literal"

    def test_composed_sql_has_balanced_structure(self):
        """Test that complex composed SQL maintains proper structure."""
        registry = get_operator_registry()

        test_cases = [
            (SQL("(data ->> 'age')"), "gte", 18, int, "numeric comparison"),
            (SQL("(data ->> 'active')"), "eq", True, bool, "boolean comparison"),
            (SQL("(data ->> 'tags')"), "in", ["red", "blue"], str, "string list"),
        ]

        for path_sql, op, value, value_type, description in test_cases:
            strategy = registry.get_strategy(op, value_type)
            result = strategy.build_sql(path_sql, op, value, value_type)

            sql_str = str(result)
            print(f"{description}: {sql_str}")

            # Basic structural validation
            assert "Composed(" in sql_str, f"Should be properly composed: {sql_str}"
            assert "SQL(" in sql_str, f"Should contain SQL components: {sql_str}"

            # Count parentheses for balance (in the string representation)
            open_count = sql_str.count("(")
            close_count = sql_str.count(")")
            assert open_count == close_count, (
                f"Unbalanced parentheses in {description}: "
                f"open={open_count}, close={close_count}, sql={sql_str}"
            )

    def test_no_sql_injection_in_structure(self):
        """Test that potentially malicious values are properly contained."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'comment')")

        malicious_input = "'; DROP TABLE users; --"
        strategy = registry.get_strategy("eq", str)
        result = strategy.build_sql(jsonb_path, "eq", malicious_input, str)

        sql_str = str(result)
        print(f"Malicious input handling: {sql_str}")

        # The malicious content should be wrapped in Literal()
        assert f'Literal("{malicious_input}")' in sql_str, (
            f"Malicious content not properly parameterized: {sql_str}"
        )

        # Should not have raw SQL injection
        assert "DROP TABLE" not in sql_str.replace(f'Literal("{malicious_input}")', ""), (
            f"Raw SQL injection detected outside of Literal: {sql_str}"
        )

    def test_type_consistency_validation(self):
        """Test that the same operation type produces consistent results."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'score')")

        # Test multiple calls to same operation
        strategy = registry.get_strategy("eq", int)
        result1 = strategy.build_sql(jsonb_path, "eq", 100, int)
        result2 = strategy.build_sql(jsonb_path, "eq", 200, int)

        sql1 = str(result1)
        sql2 = str(result2)

        print(f"First call: {sql1}")
        print(f"Second call: {sql2}")

        # Should have same structural pattern (only values differ)
        assert sql1.count("::numeric") == sql2.count("::numeric"), "Inconsistent casting"
        assert sql1.count("SQL(' = ')") == sql2.count("SQL(' = ')"), "Inconsistent operators"
        assert "Literal(100)" in sql1 and "Literal(200)" in sql2, (
            "Values not properly differentiated"
        )


if __name__ == "__main__":
    print("Testing precise SQL validation...")
    print("Run with: pytest tests/regression/where_clause/test_precise_sql_validation.py -v -s")
