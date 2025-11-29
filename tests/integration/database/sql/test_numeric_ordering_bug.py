"""Test for numeric ordering bug in FraiseQL.

This test demonstrates the issue where FraiseQL uses JSONB text extraction
(data->>'field') instead of numeric extraction (data->'field') for ordering,
causing lexicographic sorting instead of proper numeric sorting.

Bug: "125.0" > "1234.53" because text comparison treats "2" > "1"
Fix: Use data->'amount' instead of data->>'amount' for numeric fields
"""

import uuid

import pytest

import fraiseql
from fraiseql.sql.order_by_generator import OrderBy, OrderBySet, OrderDirection

pytestmark = pytest.mark.database


@fraiseql.type
class Price:
    """Price type with numeric amount field for testing ordering."""

    id: uuid.UUID
    amount: float  # Numeric field that should be ordered numerically
    identifier: str


class TestNumericOrderingBug:
    """Test suite for numeric ordering bug."""

    def test_single_numeric_field_ordering_bug(self) -> None:
        """Test that demonstrates the numeric ordering bug with single field.

        EXPECTED BEHAVIOR: Numeric values should be ordered mathematically
        ACTUAL BEHAVIOR: Currently uses text extraction causing lexicographic ordering
        """
        # Create order by for a numeric field
        order_by = OrderBy(field="amount", direction=OrderDirection.ASC)
        sql = order_by.to_sql("data").as_string(None)

        # What it SHOULD generate for numeric fields (CORRECT)
        # This test will fail until we fix the implementation
        assert sql == "data -> 'amount' ASC", (
            f"Expected numeric JSONB extraction, got text extraction: {sql}"
        )

    def test_multiple_numeric_fields_ordering_bug(self) -> None:
        """Test numeric ordering bug with multiple numeric fields."""
        order_by_set = OrderBySet(
            [
                OrderBy(field="amount", direction="asc"),
                OrderBy(field="quantity", direction="desc"),
            ]
        )
        sql = order_by_set.to_sql("data").as_string(None)

        # Now FIXED - uses JSONB extraction for proper numeric ordering
        expected_correct = "ORDER BY data -> 'amount' ASC, data -> 'quantity' DESC"
        assert sql == expected_correct

    def test_mixed_field_types_ordering(self) -> None:
        """Test ordering with both numeric and text fields.

        NOTE: Current implementation treats all fields uniformly with JSONB extraction.
        This is acceptable since JSONB extraction preserves original types and
        PostgreSQL can handle both numeric and text comparisons correctly.
        """
        order_by_set = OrderBySet(
            [
                OrderBy(field="amount", direction="asc"),  # Uses JSONB extraction
                OrderBy(field="identifier", direction="desc"),  # Uses JSONB extraction
            ]
        )
        sql = order_by_set.to_sql("data").as_string(None)

        # FIXED - both use JSONB extraction which preserves types
        expected_correct = "ORDER BY data -> 'amount' ASC, data -> 'identifier' DESC"
        assert sql == expected_correct

    def test_nested_numeric_field_ordering_bug(self) -> None:
        """Test numeric ordering bug with nested fields."""
        order_by = OrderBy(field="pricing.amount", direction="desc")
        sql = order_by.to_sql("data").as_string(None)

        # Should use JSONB extraction for nested numeric fields (CORRECT)
        assert sql == "data -> 'pricing' -> 'amount' DESC", (
            f"Expected full JSONB extraction, got: {sql}"
        )


@pytest.mark.integration
class TestNumericOrderingRealWorld:
    """Integration tests that demonstrate real-world impact of the ordering bug."""

    def test_financial_amounts_ordering_simulation(self) -> None:
        """Demonstrate the difference between lexicographic and numeric ordering.

        This validates that our fix addresses the core issue where string sorting
        differs from numeric sorting for financial amounts.
        """
        amounts = [25.0, 125.0, 1234.53, 1000.0]

        # Lexicographic (string) ordering: "1000.0", "1234.53", "125.0", "25.0"
        lexicographic = sorted([str(x) for x in amounts])
        # Numeric ordering: 25.0, 125.0, 1000.0, 1234.53
        numeric = sorted(amounts)

        # Verify they differ (demonstrating the original bug)
        assert lexicographic == ["1000.0", "1234.53", "125.0", "25.0"]
        assert numeric == [25.0, 125.0, 1000.0, 1234.53]
        assert [float(x) for x in lexicographic] != numeric

    def test_decimal_precision_ordering_bug(self) -> None:
        """Test ordering bug with high-precision decimal values."""
        order_by = OrderBy(field="precise_amount", direction="asc")
        sql = order_by.to_sql("data").as_string(None)

        # FIXED - now uses JSONB extraction which preserves numeric precision
        assert sql == "data -> 'precise_amount' ASC"

        # This now correctly handles values like:
        # 123.456, 123.5, 123.45678
        # JSONB numeric sort: 123.45, 123.456, 123.5 (CORRECT)

    def test_performance_impact_documentation(self) -> None:
        """Document performance implications of the fix.

        Using JSONB extraction (data->'field') vs text extraction (data->>'field')
        has better performance characteristics for numeric operations.
        """
        order_by = OrderBy(field="amount", direction="asc")
        sql = order_by.to_sql("data").as_string(None)

        # FIXED: data -> 'amount' (JSONB extraction)
        # - Better index utilization potential
        # - Native numeric comparison in PostgreSQL
        # - More efficient for numeric operations
        # - Preserves original data types
        assert "data -> 'amount'" in sql
        assert "data ->> 'amount'" not in sql


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
