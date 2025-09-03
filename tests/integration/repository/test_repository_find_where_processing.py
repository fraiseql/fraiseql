"""Test FraiseQLRepository.find() WHERE clause processing integration.

This test reproduces the bug where FraiseQLRepository.find() uses primitive
SQL templates instead of FraiseQL's operator strategy system for WHERE clauses.
"""

import pytest
from unittest.mock import Mock, patch

from fraiseql.db import FraiseQLRepository


class TestRepositoryFindWhereProcessing:
    """Test that FraiseQLRepository.find() uses FraiseQL's WHERE generation system."""

    def test_repository_find_uses_primitive_sql_templates_not_operator_strategies(self):
        """TEST THAT WILL FAIL: Repository bypasses operator strategies.

        This test demonstrates that FraiseQLRepository.find() uses primitive
        SQL templates instead of the sophisticated operator strategy system
        that contains the IP filtering fixes.
        """
        # Create a mock pool
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test the _build_dict_where_condition method directly
        # This is the problematic method that uses hardcoded SQL
        where_condition = db._build_dict_where_condition(
            field_name="ip_address",
            operator="eq",
            value="192.168.1.1"
        )

        # Convert to string to check the generated SQL
        condition_str = str(where_condition)

        # This will PASS but it's wrong! It should use ::inet casting for IP addresses
        # The current implementation generates: (data->>'ip_address') = '192.168.1.1'
        # It should generate: (data->>'ip_address')::inet = '192.168.1.1'::inet

        # For now, let's just verify it generates some SQL
        assert where_condition is not None, "Should generate SQL condition"
        assert "ip_address" in condition_str, "Should contain field name"
        assert "192.168.1.1" in condition_str, "Should contain value"

        # This is what we expect to be TRUE but currently is FALSE:
        # The primitive template does NOT use INET casting
        has_inet_casting = "::inet" in condition_str

        print(f"Generated SQL: {condition_str}")
        print(f"Has INET casting: {has_inet_casting}")

        # This assertion will FAIL, proving the bug
        # The find() method should use operator strategies, not primitive templates
        assert has_inet_casting, (
            f"Repository should use operator strategies with INET casting for IP addresses, "
            f"but got primitive SQL: {condition_str}"
        )

    @pytest.mark.asyncio
    async def test_repository_find_should_use_operator_strategy_system(self, db_pool):
        """TEST SHOWING THE CORRECT BEHAVIOR: find() should use operator strategies."""
        db = FraiseQLRepository(db_pool)

        # Mock the operator strategy system to show it should be called
        with patch('fraiseql.sql.operator_strategies.get_operator_registry') as mock_registry:
            mock_strategy = Mock()
            mock_strategy.build_sql.return_value = Mock()  # Mock SQL result

            mock_registry_instance = Mock()
            mock_registry_instance.get_strategy.return_value = mock_strategy
            mock_registry.return_value = mock_registry_instance

            # This will fail because find() doesn't call operator strategies
            where_input = {"ipAddress": {"eq": "192.168.1.1"}}

            try:
                await db.find(
                    "test_view",
                    tenant_id="test-tenant",
                    where=where_input,
                    limit=1
                )
            except Exception:
                # Expected to fail due to missing view, that's fine
                pass

            # This assertion will FAIL because find() doesn't use operator strategies
            # It uses primitive SQL templates instead
            mock_registry.assert_called(), (
                "find() should use get_operator_registry() to get operator strategies "
                "instead of primitive SQL templates"
            )

    def test_operator_strategy_system_provides_intelligent_casting(self):
        """Show that the fixed system provides intelligent type casting."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test different operators to show intelligent type handling
        test_cases = [
            ("ip_address", "eq", "192.168.1.1", "::inet"),
            ("ip_address", "in", ["192.168.1.1", "10.0.0.1"], "::inet"),
            ("mac_address", "eq", "aa:bb:cc:dd:ee:ff", None),  # MACs don't get auto-casting yet
            ("port", "gt", 8080, None),  # Regular fields don't need casting
        ]

        for field, op, value, expected_cast in test_cases:
            condition = db._build_dict_where_condition(field, op, value)
            condition_str = str(condition) if condition else "None"

            print(f"\nField: {field}, Op: {op}, Value: {value}")
            print(f"Generated: {condition_str}")

            # Check for expected type casting behavior
            has_expected_casting = expected_cast is None or expected_cast in condition_str
            print(f"Has expected casting ({expected_cast}): {has_expected_casting}")

            # Verify intelligent casting behavior
            if expected_cast:
                assert expected_cast in condition_str, f"Expected {expected_cast} casting for {field} but got: {condition_str}"
            else:
                # For non-cast fields, just verify we get some valid SQL
                assert condition_str != "None", f"Should generate valid SQL for {field}"

    def test_operator_strategy_system_works_correctly(self):
        """Show that the operator strategy system (when used) works correctly."""
        from fraiseql.sql.operator_strategies import get_operator_registry
        from psycopg.sql import SQL

        # This is what the repository SHOULD be using
        registry = get_operator_registry()
        field_path = SQL("(data->>'ip_address')")

        # Test IP address with eq operator
        strategy = registry.get_strategy("eq", field_type=None)
        sql = strategy.build_sql(field_path, "eq", "192.168.1.1", field_type=None)
        sql_str = str(sql)

        print(f"\nCorrect operator strategy SQL: {sql_str}")

        # This should have INET casting (v0.7.1 fix)
        has_inet_casting = "::inet" in sql_str
        print(f"Has INET casting: {has_inet_casting}")

        # This should PASS - showing the operator strategy system works
        assert has_inet_casting, f"Operator strategy should use INET casting: {sql_str}"

        # This proves that the fix exists and works - the repository just isn't using it!
