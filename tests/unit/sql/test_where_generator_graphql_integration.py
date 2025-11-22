"""Tests for WHERE generator GraphQL integration.

This test suite verifies that the WHERE generator properly integrates with
GraphQL field type extraction to enable network operator functionality.
"""

from dataclasses import dataclass
from typing import get_type_hints
from unittest.mock import Mock

import pytest

from fraiseql.sql.where_generator import (
    _build_where_to_sql,
    create_where_type_with_graphql_context,
)
from fraiseql.types import IpAddress, MacAddress


@dataclass
class MockNetworkEntity:
    """Mock entity with network fields for testing."""

    id: int
    name: str
    ip_address: IpAddress
    mac_address: MacAddress


class TestWhereGeneratorGraphQLIntegration:
    """Test WHERE generator integration with GraphQL field type extraction."""

    def test_create_where_type_with_graphql_context(self) -> None:
        """Test creation of WHERE type with GraphQL context support."""
        # Mock GraphQL info
        mock_info = Mock()
        mock_info.field_definition = None

        # Create WHERE type with GraphQL context
        where_type = create_where_type_with_graphql_context(MockNetworkEntity, mock_info)

        # Should create a valid dataclass
        assert hasattr(where_type, "__dataclass_fields__")
        assert hasattr(where_type, "to_sql")

        # Should have fields for all entity attributes
        instance = where_type()
        assert hasattr(instance, "id")
        assert hasattr(instance, "name")
        assert hasattr(instance, "ip_address")
        assert hasattr(instance, "mac_address")

    def test_where_type_graphql_context_field_extraction(self) -> None:
        """Test that WHERE type uses GraphQL context for field type extraction."""
        # Mock GraphQL info
        mock_info = Mock()
        mock_info.field_definition = None

        # Create WHERE type with GraphQL context
        where_type = create_where_type_with_graphql_context(MockNetworkEntity, mock_info)
        instance = where_type()

        # Set up a network filter using camelCase (GraphQL style)
        instance.ip_address = {"eq": "8.8.8.8"}

        # Generate SQL
        sql = instance.to_sql()

        # Should generate valid SQL
        assert sql is not None
        sql_str = str(sql)

        # Should contain the field name and operator
        assert "ip_address" in sql_str
        assert "8.8.8.8" in sql_str

    def test_build_where_to_sql_with_graphql_context(self) -> None:
        """Test _build_where_to_sql with GraphQL context parameter."""
        # Mock GraphQL info
        mock_info = Mock()
        mock_info.field_definition = None

        # Get type hints from our mock entity
        type_hints = get_type_hints(MockNetworkEntity)
        field_names = list(type_hints.keys())

        # Build to_sql function with GraphQL context
        to_sql_func = _build_where_to_sql(field_names, type_hints, mock_info)

        # Should return a callable
        assert callable(to_sql_func)

        # Create a mock filter instance
        mock_filter = Mock()
        mock_filter.id = None
        mock_filter.name = None
        mock_filter.ip_address = {"eq": "192.168.1.1"}
        mock_filter.mac_address = None

        # Call the to_sql function directly
        sql = to_sql_func(mock_filter)

        # Should generate SQL
        assert sql is not None
        sql_str = str(sql)

        # Should contain IP address filter
        assert "192.168.1.1" in sql_str

    def test_graphql_context_enhances_field_type_detection(self) -> None:
        """Test that GraphQL context enhances field type detection beyond type hints."""

        @dataclass
        class EntityWithoutNetworkTypes:
            """Entity without explicit network type hints."""

            id: int
            server_ip: str  # Defined as str, but should be detected as IpAddress
            device_mac: str  # Defined as str, but should be detected as MacAddress

        # Mock GraphQL info
        mock_info = Mock()
        mock_info.field_definition = None

        # Create WHERE type with GraphQL context
        where_type = create_where_type_with_graphql_context(EntityWithoutNetworkTypes, mock_info)
        instance = where_type()

        # Set up network filters
        instance.server_ip = {"eq": "10.0.0.1"}
        instance.device_mac = {"eq": "00:11:22:33:44:55"}

        # Generate SQL
        sql = instance.to_sql()

        # Should generate valid SQL even though original types were str
        assert sql is not None
        sql_str = str(sql)

        # Should contain both filters
        assert "10.0.0.1" in sql_str
        assert "00:11:22:33:44:55" in sql_str

    def test_graphql_context_fallback_graceful(self) -> None:
        """Test that GraphQL context integration fails gracefully."""
        # Test with None GraphQL info
        where_type = create_where_type_with_graphql_context(MockNetworkEntity, None)
        instance = where_type()

        # Should still work without GraphQL context
        instance.ip_address = {"eq": "8.8.8.8"}
        sql = instance.to_sql()

        # Should generate SQL using type hints
        assert sql is not None
        assert "8.8.8.8" in str(sql)

    def test_backwards_compatibility_maintained(self) -> None:
        """Test that the enhancement maintains backwards compatibility."""
        from fraiseql.sql.where_generator import safe_create_where_type

        # Original function should still work
        original_where_type = safe_create_where_type(MockNetworkEntity)
        original_instance = original_where_type()

        # Should have same structure as enhanced version
        enhanced_where_type = create_where_type_with_graphql_context(MockNetworkEntity)
        enhanced_instance = enhanced_where_type()

        # Both should have the same fields
        original_fields = set(dir(original_instance))
        enhanced_fields = set(dir(enhanced_instance))

        # Enhanced version may have additional internal attributes, but should have all original ones
        original_public_fields = {f for f in original_fields if not f.startswith("_")}
        enhanced_public_fields = {f for f in enhanced_fields if not f.startswith("_")}

        assert original_public_fields.issubset(enhanced_public_fields), (
            "Enhanced WHERE type should maintain all original public fields"
        )


class TestNetworkOperatorIntegration:
    """Test integration with network operator strategies."""

    def test_network_field_uses_network_operator_strategy(self) -> None:
        """Test that network fields use NetworkOperatorStrategy through GraphQL context."""
        # This test verifies the end-to-end integration

        # Mock GraphQL context that would have network field information
        mock_info = Mock()
        mock_info.field_definition = None

        # Create WHERE type with GraphQL context
        where_type = create_where_type_with_graphql_context(MockNetworkEntity, mock_info)
        instance = where_type()

        # Set up IP address equality filter
        instance.ip_address = {"eq": "203.0.113.1"}

        # Generate SQL
        sql = instance.to_sql()
        sql_str = str(sql)

        # The SQL should be generated using NetworkOperatorStrategy
        # which uses ::inet casting instead of host() function
        assert "::inet" in sql_str, (
            f"Expected ::inet casting from NetworkOperatorStrategy, got: {sql_str}"
        )

        # Should contain the IP address
        assert "203.0.113.1" in sql_str

    def test_comparison_with_original_behavior(self) -> None:
        """Test comparison between original and enhanced behavior."""
        from fraiseql.sql.where_generator import safe_create_where_type

        # Original WHERE type (without GraphQL context)
        original_where_type = safe_create_where_type(MockNetworkEntity)
        original_instance = original_where_type()
        original_instance.ip_address = {"eq": "198.51.100.1"}
        original_sql = str(original_instance.to_sql())

        # Enhanced WHERE type (with GraphQL context)
        mock_info = Mock()
        enhanced_where_type = create_where_type_with_graphql_context(MockNetworkEntity, mock_info)
        enhanced_instance = enhanced_where_type()
        enhanced_instance.ip_address = {"eq": "198.51.100.1"}
        enhanced_sql = str(enhanced_instance.to_sql())

        # Both should generate valid SQL
        assert original_sql is not None
        assert enhanced_sql is not None

        # Enhanced version should potentially use different operator strategies
        # The key is that both work, but enhanced may have better network support
        assert "198.51.100.1" in original_sql
        assert "198.51.100.1" in enhanced_sql


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
