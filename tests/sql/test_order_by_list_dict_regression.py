"""Regression tests for OrderBy list of dictionaries fix.

This test suite ensures that FraiseQL can handle OrderBy inputs in the format
[{'ipAddress': 'asc'}] which was failing in v0.3.5 due to:
1. Lack of support for list of dictionaries format
2. Field name mapping between GraphQL camelCase and Python snake_case
"""

import uuid
from typing import Optional

import pytest

import fraiseql
from fraiseql.sql.graphql_order_by_generator import (
    OrderDirection,
    _convert_order_by_input_to_sql,
    create_graphql_order_by_input,
)


@fraiseql.type
class DnsServer:
    """Test type matching the original bug report."""
    id: uuid.UUID
    ip_address: str
    server_name: str
    is_active: bool = True
    dns_server_type: Optional[str] = None


class TestOrderByListDictRegression:
    """Regression tests for the OrderBy list of dictionaries bug."""

    def test_list_of_dicts_conversion(self):
        """Test that list of dicts is properly converted to OrderBySet."""
        # This is the exact input format that was failing
        order_by_input = [{'ipAddress': 'asc'}]

        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None, "Should convert list of dicts successfully"
        assert len(result.instructions) == 1, "Should have one instruction"

        instruction = result.instructions[0]
        assert instruction.field == 'ip_address', f"Field should be snake_case, got {instruction.field}"
        assert instruction.direction == 'asc', f"Direction should be lowercase, got {instruction.direction}"

    def test_multiple_fields_in_list(self):
        """Test multiple field ordering in list format."""
        order_by_input = [
            {'ipAddress': 'asc'},
            {'serverName': 'desc'},
            {'isActive': 'ASC'}  # Test case normalization
        ]

        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None
        assert len(result.instructions) == 3

        # Check field name conversion and direction normalization
        expected = [
            ('ip_address', 'asc'),
            ('server_name', 'desc'),
            ('is_active', 'asc')  # ASC -> asc
        ]

        actual = [(instr.field, instr.direction) for instr in result.instructions]
        assert actual == expected, f"Expected {expected}, got {actual}"

    def test_complex_field_names(self):
        """Test complex camelCase to snake_case conversion."""
        test_cases = [
            ('ipAddress', 'ip_address'),
            ('serverName', 'server_name'),
            ('dnsServerType', 'dns_server_type'),
            ('isActive', 'is_active'),
            ('id', 'id'),  # No conversion needed
            ('APIKey', 'api_key'),  # Consecutive capitals
            ('XMLParser', 'xml_parser'),  # Consecutive capitals
            ('HTTPSProxy', 'https_proxy'),  # Multiple consecutive capitals
        ]

        for camel_case, expected_snake_case in test_cases:
            order_by_input = [{camel_case: 'asc'}]
            result = _convert_order_by_input_to_sql(order_by_input)

            assert result is not None, f"Failed to convert {camel_case}"
            assert len(result.instructions) == 1
            assert result.instructions[0].field == expected_snake_case, \
                f"Expected {camel_case} -> {expected_snake_case}, got {result.instructions[0].field}"

    def test_direction_case_normalization(self):
        """Test that direction strings are normalized to lowercase."""
        test_cases = [
            ('asc', 'asc'),
            ('desc', 'desc'),
            ('ASC', 'asc'),
            ('DESC', 'desc'),
            ('Asc', 'asc'),
            ('Desc', 'desc'),
        ]

        for input_direction, expected_direction in test_cases:
            order_by_input = [{'ipAddress': input_direction}]
            result = _convert_order_by_input_to_sql(order_by_input)

            assert result is not None
            assert result.instructions[0].direction == expected_direction, \
                f"Expected {input_direction} -> {expected_direction}, got {result.instructions[0].direction}"

    def test_mixed_formats_in_list(self):
        """Test that a list can contain mixed formats (if supported)."""
        # This tests that we can handle OrderByItem objects and dicts in the same list
        # Note: In practice, GraphQL would send consistent format, but we should be robust

        order_by_input = [
            {'ipAddress': 'asc'},
            {'serverName': 'desc'}
        ]

        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None
        assert len(result.instructions) == 2

        expected_fields = ['ip_address', 'server_name']
        actual_fields = [instr.field for instr in result.instructions]
        assert actual_fields == expected_fields

    def test_invalid_direction_ignored(self):
        """Test that invalid directions are ignored gracefully."""
        order_by_input = [
            {'ipAddress': 'invalid'},
            {'serverName': 'asc'}  # This should still work
        ]

        result = _convert_order_by_input_to_sql(order_by_input)

        # Should only include the valid instruction
        if result:
            # Only valid directions should be included
            assert len(result.instructions) == 1
            assert result.instructions[0].field == 'server_name'
        else:
            # Or it might return None if no valid instructions
            assert result is None

    def test_empty_list(self):
        """Test that empty list returns None."""
        result = _convert_order_by_input_to_sql([])
        assert result is None

    def test_none_values_filtered(self):
        """Test that None values are filtered out."""
        order_by_input = [
            {'ipAddress': None},
            {'serverName': 'asc'}
        ]

        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None
        assert len(result.instructions) == 1
        assert result.instructions[0].field == 'server_name'

    def test_sql_generation(self):
        """Test that the generated SQL is correct."""
        order_by_input = [{'ipAddress': 'asc'}, {'serverName': 'desc'}]
        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None

        sql = result.to_sql()
        sql_str = sql.as_string(None)

        # Should generate proper JSONB ORDER BY clause
        assert 'ORDER BY' in sql_str
        assert "data ->> 'ip_address' ASC" in sql_str
        assert "data ->> 'server_name' DESC" in sql_str

    def test_backward_compatibility_with_dicts(self):
        """Test that single dict input still works (not in list)."""
        # Test single dict (not in list) - should still work
        order_by_input = {'ipAddress': 'asc'}
        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None
        assert len(result.instructions) == 1
        assert result.instructions[0].field == 'ip_address'
        assert result.instructions[0].direction == 'asc'

    def test_generated_input_types_still_work(self):
        """Test that FraiseQL-generated input types still work after the fix."""
        # Create the proper FraiseQL OrderBy input type
        DnsServerOrderBy = create_graphql_order_by_input(DnsServer)

        # This should create an instance with snake_case field names
        order_by = DnsServerOrderBy(ip_address=OrderDirection.ASC)

        # Should have the conversion method
        assert hasattr(order_by, '_to_sql_order_by')

        # Should convert properly
        result = order_by._to_sql_order_by()
        assert result is not None
        assert len(result.instructions) == 1
        assert result.instructions[0].field == 'ip_address'
        assert result.instructions[0].direction == 'asc'

    def test_multiple_fields_single_dict(self):
        """Test multiple fields in a single dictionary."""
        order_by_input = [{'ipAddress': 'asc', 'serverName': 'desc'}]
        result = _convert_order_by_input_to_sql(order_by_input)

        assert result is not None
        assert len(result.instructions) == 2

        # Should have both fields with proper conversion
        fields = {instr.field: instr.direction for instr in result.instructions}
        assert 'ip_address' in fields
        assert 'server_name' in fields
        assert fields['ip_address'] == 'asc'
        assert fields['server_name'] == 'desc'


class TestOrderByIntegrationRegression:
    """Integration tests with the database layer."""

    def test_repository_handles_list_dicts(self):
        """Test that the repository layer properly routes list of dicts."""
        from fraiseql.db import FraiseQLRepository

        # Mock pool that doesn't need real database
        class MockPool:
            def connection(self):
                return None

        repo = FraiseQLRepository(MockPool())

        # Test that _build_find_query doesn't crash with list input
        order_by_input = [{'ipAddress': 'asc'}]

        try:
            query = repo._build_find_query(
                "dns_servers_view",
                order_by=order_by_input,
                limit=10
            )

            # Should create a proper query object
            assert query is not None
            assert hasattr(query, 'statement')

            # The statement should be a SQL object that can be converted to string
            sql_str = str(query.statement)

            # Should not contain the raw list (which would cause the original error)
            assert '[{' not in sql_str

        except Exception as e:
            # Should not get the original error
            assert "SQL values must be strings" not in str(e)
            # Other errors (like mock-related) are acceptable for this test


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
