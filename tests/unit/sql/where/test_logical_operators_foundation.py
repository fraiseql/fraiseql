"""Test logical operators foundation for issue #33.

These tests demonstrate the clean foundation for implementing
OR/AND/NOT operators in GraphQL where clauses.
"""

import pytest
from psycopg.sql import SQL, Composed
from fraiseql.sql.where.operators.logical import build_and_sql, build_or_sql, build_not_sql


class TestLogicalOperatorsFoundation:
    """Test logical operators foundation functionality."""

    def test_build_and_sql_with_multiple_conditions(self):
        """Should combine multiple conditions with AND."""
        condition1 = Composed([SQL("field1 = 'value1'")])
        condition2 = Composed([SQL("field2 = 'value2'")])
        condition3 = Composed([SQL("field3 = 'value3'")])

        result = build_and_sql([condition1, condition2, condition3])
        sql_str = result.as_string(None)

        assert "field1 = 'value1'" in sql_str
        assert "field2 = 'value2'" in sql_str
        assert "field3 = 'value3'" in sql_str
        assert " AND " in sql_str
        assert sql_str.startswith("(")
        assert sql_str.endswith(")")

    def test_build_and_sql_with_single_condition(self):
        """Should return single condition as-is."""
        condition = Composed([SQL("field1 = 'value1'")])
        result = build_and_sql([condition])

        assert result == condition

    def test_build_and_sql_with_empty_conditions(self):
        """Should return TRUE for empty conditions."""
        result = build_and_sql([])
        sql_str = result.as_string(None)

        assert sql_str == "TRUE"

    def test_build_or_sql_with_multiple_conditions(self):
        """Should combine multiple conditions with OR."""
        condition1 = Composed([SQL("status = 'draft'")])
        condition2 = Composed([SQL("status = 'published'")])

        result = build_or_sql([condition1, condition2])
        sql_str = result.as_string(None)

        assert "status = 'draft'" in sql_str
        assert "status = 'published'" in sql_str
        assert " OR " in sql_str
        assert sql_str.startswith("(")
        assert sql_str.endswith(")")

    def test_build_or_sql_with_single_condition(self):
        """Should return single condition as-is."""
        condition = Composed([SQL("field1 = 'value1'")])
        result = build_or_sql([condition])

        assert result == condition

    def test_build_or_sql_with_empty_conditions(self):
        """Should return FALSE for empty conditions."""
        result = build_or_sql([])
        sql_str = result.as_string(None)

        assert sql_str == "FALSE"

    def test_build_not_sql(self):
        """Should negate a condition with NOT."""
        condition = Composed([SQL("status = 'archived'")])
        result = build_not_sql(condition)
        sql_str = result.as_string(None)

        assert "NOT (" in sql_str
        assert "status = 'archived'" in sql_str
        assert sql_str.endswith(")")

    def test_complex_nested_logical_operations(self):
        """Should handle complex nested logical operations."""
        # Simulate: (field1 = 'a' OR field1 = 'b') AND (field2 = 'c')
        condition1 = Composed([SQL("field1 = 'a'")])
        condition2 = Composed([SQL("field1 = 'b'")])
        condition3 = Composed([SQL("field2 = 'c'")])

        # Build OR condition first
        or_condition = build_or_sql([condition1, condition2])

        # Then AND with third condition
        final_condition = build_and_sql([or_condition, condition3])
        sql_str = final_condition.as_string(None)

        assert "field1 = 'a'" in sql_str
        assert "field1 = 'b'" in sql_str
        assert "field2 = 'c'" in sql_str
        assert " OR " in sql_str
        assert " AND " in sql_str

    def test_not_with_complex_condition(self):
        """Should handle NOT with complex nested conditions."""
        # Simulate: NOT (field1 = 'a' AND field2 = 'b')
        condition1 = Composed([SQL("field1 = 'a'")])
        condition2 = Composed([SQL("field2 = 'b'")])

        and_condition = build_and_sql([condition1, condition2])
        not_condition = build_not_sql(and_condition)
        sql_str = not_condition.as_string(None)

        assert "NOT (" in sql_str
        assert "field1 = 'a'" in sql_str
        assert "field2 = 'b'" in sql_str
        assert " AND " in sql_str
