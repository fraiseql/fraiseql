"""Test JSONB network filtering issue reproduction.

This module reproduces the specific issue described in the bug report
where NetworkAddressFilter appears to have inconsistent behavior when
filtering IP addresses stored in JSONB columns.
"""

from dataclasses import dataclass
from typing import get_type_hints

import pytest

from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql.types import IpAddress

pytestmark = pytest.mark.database


@dataclass
class DnsServer:
    """Test DNS server with IP address fields stored in JSONB."""

    id: str
    identifier: str
    ip_address: IpAddress
    n_total_allocations: int | None = None


class TestJSONBNetworkFilteringBug:
    """Reproduce the JSONB network filtering bug described in the issue."""

    def test_where_input_generation_has_network_operators(self) -> None:
        """Test that DnsServer generates proper where input with network operators."""
        WhereInput = create_graphql_where_input(DnsServer)

        # Verify the types
        type_hints = get_type_hints(WhereInput)

        # IP address field should have network operators
        ip_filter_type = type_hints["ip_address"]
        if hasattr(ip_filter_type, "__args__") and ip_filter_type.__args__:
            filter_class = ip_filter_type.__args__[0]
        else:
            filter_class = ip_filter_type

        filter_instance = filter_class()

        # Check that network operators are available
        assert hasattr(filter_instance, "eq"), "Basic eq operator should be available"
        assert hasattr(filter_instance, "inSubnet"), "Network inSubnet operator should be available"
        assert hasattr(filter_instance, "isPrivate"), (
            "Network isPrivate operator should be available"
        )
        assert hasattr(filter_instance, "isPublic"), "Network isPublic operator should be available"


class TestRootCauseInvestigation:
    """Additional tests to investigate the root cause."""

    def test_sql_generation_for_jsonb_network_field(self) -> None:
        """Test SQL generation for network filtering on JSONB fields."""
        from psycopg.sql import SQL

        from fraiseql.sql.operators import NetworkOperatorStrategy
        from fraiseql.types import IpAddress

        strategy = NetworkOperatorStrategy()

        # Test subnet operation SQL generation
        field_path = SQL("data->>'ip_address'")
        result = strategy.build_sql("inSubnet", "192.168.1.0/24", field_path, field_type=IpAddress)

        # Should generate proper PostgreSQL inet subnet matching
        sql_str = result.as_string(None)  # type: ignore

        # Check that it properly casts JSONB text to inet
        assert "::inet" in sql_str, "Should cast to inet type"
        assert "<<=" in sql_str, "Should use PostgreSQL subnet operator"
        assert "192.168.1.0/24" in sql_str, "Should include subnet parameter"

    def test_eq_operator_sql_generation(self) -> None:
        """Test SQL generation for eq operator on JSONB network field."""
        from psycopg.sql import SQL

        from fraiseql.sql.operators import StringOperatorStrategy as ComparisonOperatorStrategy
        from fraiseql.types import IpAddress

        strategy = ComparisonOperatorStrategy()

        # Test exact matching SQL generation
        field_path = SQL("data->>'ip_address'")
        result = strategy.build_sql("eq", "1.1.1.1", field_path, field_type=IpAddress)

        sql_str = str(result)

        # Should properly handle IP address comparison
        # The exact format may vary based on IP address handling
        assert "1.1.1.1" in sql_str, "Should include IP address value"
