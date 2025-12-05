"""Test suite for the new nested array resolver with where filtering support."""

import uuid
from typing import Optional

import pytest

from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where
from fraiseql.fields import fraise_field
from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql.types import fraise_type


@fraise_type
class MockPrintServer:
    id: uuid.UUID
    hostname: str
    ip_address: Optional[str] = None
    operating_system: str
    n_total_allocations: int = 0


@fraise_type
class MockNetworkConfiguration:
    id: uuid.UUID
    identifier: str
    name: str
    print_servers: list[MockPrintServer] = fraise_field(
        default_factory=list, supports_where_filtering=True, nested_where_type=MockPrintServer
    )


@pytest.mark.unit
class TestNestedArrayResolverWithWhere:
    """Test the enhanced nested array resolver with where filtering."""

    @pytest.fixture
    def sample_network_config(self) -> None:
        """Create a sample network configuration with print servers."""
        return MockNetworkConfiguration(
            id=uuid.uuid4(),
            identifier="test-network",
            name="Test Network",
            print_servers=[
                MockPrintServer(
                    id=uuid.uuid4(),
                    hostname="prod-server-01",
                    ip_address="192.168.1.10",
                    operating_system="Windows Server",
                    n_total_allocations=150,
                ),
                MockPrintServer(
                    id=uuid.uuid4(),
                    hostname="dev-server-01",
                    ip_address="192.168.1.20",
                    operating_system="Linux",
                    n_total_allocations=25,
                ),
                MockPrintServer(
                    id=uuid.uuid4(),
                    hostname="prod-server-02",
                    ip_address=None,  # Offline server
                    operating_system="Windows Server",
                    n_total_allocations=0,
                ),
            ],
        )

    @pytest.mark.asyncio
    async def test_resolver_without_where_returns_all_items(self, sample_network_config) -> None:
        """Test that resolver returns all items when no where filter is provided."""
        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None)

        assert len(result) == 3
        assert all(isinstance(server, MockPrintServer) for server in result)

    @pytest.mark.asyncio
    async def test_resolver_with_hostname_filter(self, sample_network_config) -> None:
        """Test filtering by hostname."""
        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)

        where_filter = PrintServerWhereInput()
        where_filter.hostname = {"contains": "prod"}

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None, where=where_filter)

        assert len(result) == 2  # Both prod servers
        hostnames = [server.hostname for server in result]
        assert "prod-server-01" in hostnames
        assert "prod-server-02" in hostnames
        assert "dev-server-01" not in hostnames

    @pytest.mark.asyncio
    async def test_resolver_with_ip_address_filter(self, sample_network_config) -> None:
        """Test filtering by IP address (null/not null)."""
        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)

        # Filter for servers with IP addresses (not null)
        where_filter = PrintServerWhereInput()
        where_filter.ip_address = {"isnull": False}

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None, where=where_filter)

        assert len(result) == 2  # Only servers with IPs
        for server in result:
            assert server.ip_address is not None

    @pytest.mark.asyncio
    async def test_resolver_with_numeric_range_filter(self, sample_network_config) -> None:
        """Test filtering by numeric range."""
        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)

        where_filter = PrintServerWhereInput()
        where_filter.n_total_allocations = {"gte": 50}

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None, where=where_filter)

        assert len(result) == 1  # Only the server with 150 allocations
        assert result[0].hostname == "prod-server-01"
        assert result[0].n_total_allocations == 150

    @pytest.mark.asyncio
    async def test_resolver_with_enum_filter(self, sample_network_config) -> None:
        """Test filtering by enum/choice values."""
        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)

        where_filter = PrintServerWhereInput()
        where_filter.operating_system = {"in_": ["Linux", "macOS"]}

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None, where=where_filter)

        assert len(result) == 1  # Only the Linux server
        assert result[0].operating_system == "Linux"
        assert result[0].hostname == "dev-server-01"

    @pytest.mark.asyncio
    async def test_resolver_with_multiple_filters(self, sample_network_config) -> None:
        """Test filtering with multiple criteria (AND logic)."""
        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)

        where_filter = PrintServerWhereInput()
        where_filter.hostname = {"startswith": "prod"}
        where_filter.ip_address = {"isnull": False}  # Must have IP
        where_filter.n_total_allocations = {"gt": 0}  # Must have allocations

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None, where=where_filter)

        # Should only return prod-server-01 (has IP and allocations)
        assert len(result) == 1
        assert result[0].hostname == "prod-server-01"
        assert result[0].ip_address == "192.168.1.10"
        assert result[0].n_total_allocations == 150

    @pytest.mark.asyncio
    async def test_resolver_returns_empty_list_when_no_matches(self, sample_network_config) -> None:
        """Test that resolver returns empty list when no items match."""
        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)

        where_filter = PrintServerWhereInput()
        where_filter.hostname = {"contains": "nonexistent"}

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(sample_network_config, None, where=where_filter)

        assert result == []

    @pytest.mark.asyncio
    async def test_resolver_handles_empty_array(self) -> None:
        """Test that resolver handles empty array gracefully."""
        empty_config = MockNetworkConfiguration(
            id=uuid.uuid4(), identifier="empty-network", name="Empty Network", print_servers=[]
        )

        PrintServerWhereInput = create_graphql_where_input(MockPrintServer)
        where_filter = PrintServerWhereInput()
        where_filter.hostname = {"contains": "anything"}

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(empty_config, None, where=where_filter)

        assert result == []

    @pytest.mark.asyncio
    async def test_resolver_handles_none_value(self) -> None:
        """Test that resolver handles None field value."""
        config_with_none = MockNetworkConfiguration(
            id=uuid.uuid4(), identifier="none-network", name="None Network", print_servers=None
        )

        resolver = create_nested_array_field_resolver_with_where(
            "print_servers", list[MockPrintServer]
        )

        result = await resolver(config_with_none, None)

        # Should return empty list for list types
        assert result == []
