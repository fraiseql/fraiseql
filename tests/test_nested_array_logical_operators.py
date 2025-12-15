#!/usr/bin/env python3
"""Comprehensive tests for AND/OR/NOT logical operators in nested array filtering.

This test module specifically covers complex logical operator scenarios that can be
used with nested array where filtering, ensuring all combinations work correctly.
"""

import uuid

import pytest

from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where
from fraiseql.fields import fraise_field
from fraiseql.nested_array_filters import (
    auto_nested_array_filters,
    clear_registry,
    register_nested_array_filter,
)
from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql.types import fraise_type


@fraise_type
class Server:
    """Test server type with various fields for complex filtering."""

    id: uuid.UUID
    hostname: str
    ip_address: str | None = None
    status: str = "active"  # active, inactive, maintenance, error
    cpu_cores: int = 4
    memory_gb: int = 16
    environment: str = "production"  # production, staging, development
    is_virtual: bool = False


@auto_nested_array_filters
@fraise_type(sql_source="tv_datacenter", jsonb_column="data")
class DataCenter:
    """Test datacenter with multiple servers for complex filtering scenarios."""

    id: uuid.UUID
    name: str
    region: str
    servers: list[Server] = fraise_field(default_factory=list)


class TestNestedArrayLogicalOperators:
    """Test complex logical operators (AND/OR/NOT) in nested array filtering."""

    def setup_method(self) -> None:
        """Set up test data and clear registry."""
        clear_registry()
        register_nested_array_filter(DataCenter, "servers", Server)

        # Create comprehensive test data
        self.test_datacenter = DataCenter(
            id=uuid.uuid4(),
            name="Test DC",
            region="us-east-1",
            servers=[
                # Production servers
                Server(
                    id=uuid.uuid4(),
                    hostname="prod-web-01",
                    ip_address="10.0.1.10",
                    status="active",
                    cpu_cores=8,
                    memory_gb=32,
                    environment="production",
                    is_virtual=False,
                ),
                Server(
                    id=uuid.uuid4(),
                    hostname="prod-web-02",
                    ip_address="10.0.1.11",
                    status="active",
                    cpu_cores=8,
                    memory_gb=32,
                    environment="production",
                    is_virtual=True,
                ),
                Server(
                    id=uuid.uuid4(),
                    hostname="prod-db-01",
                    ip_address="10.0.2.10",
                    status="maintenance",
                    cpu_cores=16,
                    memory_gb=64,
                    environment="production",
                    is_virtual=False,
                ),
                # Staging servers
                Server(
                    id=uuid.uuid4(),
                    hostname="staging-web-01",
                    ip_address="10.0.3.10",
                    status="active",
                    cpu_cores=4,
                    memory_gb=16,
                    environment="staging",
                    is_virtual=True,
                ),
                Server(
                    id=uuid.uuid4(),
                    hostname="staging-db-01",
                    ip_address=None,  # Offline server
                    status="inactive",
                    cpu_cores=4,
                    memory_gb=16,
                    environment="staging",
                    is_virtual=True,
                ),
                # Development servers
                Server(
                    id=uuid.uuid4(),
                    hostname="dev-test-01",
                    ip_address="10.0.4.10",
                    status="error",
                    cpu_cores=2,
                    memory_gb=8,
                    environment="development",
                    is_virtual=True,
                ),
                Server(
                    id=uuid.uuid4(),
                    hostname="dev-build-01",
                    ip_address="10.0.4.11",
                    status="active",
                    cpu_cores=4,
                    memory_gb=16,
                    environment="development",
                    is_virtual=False,
                ),
            ],
        )

    @pytest.mark.asyncio
    async def test_simple_and_conditions_implicit(self) -> None:
        """Test implicit AND behavior (multiple fields at same level)."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Multiple conditions at same level = implicit AND
        where_filter = ServerWhereInput()
        where_filter.environment = {"equals": "production"}
        where_filter.status = {"equals": "active"}
        where_filter.is_virtual = {"equals": False}

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find: prod-web-01 (production + active + not virtual)
        assert len(result) == 1
        assert result[0].hostname == "prod-web-01"
        assert result[0].environment == "production"
        assert result[0].status == "active"
        assert result[0].is_virtual is False

    @pytest.mark.asyncio
    async def test_explicit_and_conditions(self) -> None:
        """Test explicit AND logical operator."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Explicit AND conditions
        condition1 = ServerWhereInput()
        condition1.environment = {"equals": "production"}

        condition2 = ServerWhereInput()
        condition2.cpu_cores = {"gte": 8}

        condition3 = ServerWhereInput()
        condition3.status = {"in": ["active", "maintenance"]}

        where_filter = ServerWhereInput()
        where_filter.AND = [condition1, condition2, condition3]

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find: prod-web-01, prod-web-02, prod-db-01
        # (all production with >= 8 cores and status in [active, maintenance])
        assert len(result) == 3
        hostnames = {server.hostname for server in result}
        assert hostnames == {"prod-web-01", "prod-web-02", "prod-db-01"}

    @pytest.mark.asyncio
    async def test_or_conditions(self) -> None:
        """Test OR logical operator."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # OR conditions - high-spec OR development servers
        condition1 = ServerWhereInput()
        condition1.memory_gb = {"gte": 32}  # High memory

        condition2 = ServerWhereInput()
        condition2.environment = {"equals": "development"}  # OR development

        where_filter = ServerWhereInput()
        where_filter.OR = [condition1, condition2]

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find: prod-web-01, prod-web-02, prod-db-01 (>=32GB) + dev-test-01, dev-build-01 (development)
        assert len(result) == 5
        hostnames = {server.hostname for server in result}
        assert hostnames == {
            "prod-web-01",
            "prod-web-02",
            "prod-db-01",  # High memory
            "dev-test-01",
            "dev-build-01",  # Development
        }

    @pytest.mark.asyncio
    async def test_not_conditions(self) -> None:
        """Test NOT logical operator."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # NOT condition - not production environment
        not_condition = ServerWhereInput()
        not_condition.environment = {"equals": "production"}

        where_filter = ServerWhereInput()
        where_filter.NOT = not_condition

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find all non-production servers: staging + development
        assert len(result) == 4
        hostnames = {server.hostname for server in result}
        assert hostnames == {
            "staging-web-01",
            "staging-db-01",  # Staging
            "dev-test-01",
            "dev-build-01",  # Development
        }

    @pytest.mark.asyncio
    async def test_complex_and_or_combination(self) -> None:
        """Test complex combination of AND + OR operators."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Complex: (production OR staging) AND (active OR maintenance) AND has_ip
        env_condition = ServerWhereInput()
        env_condition.OR = [
            ServerWhereInput(environment={"equals": "production"}),
            ServerWhereInput(environment={"equals": "staging"}),
        ]

        status_condition = ServerWhereInput()
        status_condition.OR = [
            ServerWhereInput(status={"equals": "active"}),
            ServerWhereInput(status={"equals": "maintenance"}),
        ]

        ip_condition = ServerWhereInput()
        ip_condition.ip_address = {"isnull": False}

        where_filter = ServerWhereInput()
        where_filter.AND = [env_condition, status_condition, ip_condition]

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find: prod-web-01, prod-web-02, prod-db-01, staging-web-01
        # (production/staging + active/maintenance + has IP)
        assert len(result) == 4
        hostnames = {server.hostname for server in result}
        assert hostnames == {
            "prod-web-01",
            "prod-web-02",
            "prod-db-01",  # Production
            "staging-web-01",  # Staging (staging-db-01 has no IP)
        }

    @pytest.mark.asyncio
    async def test_nested_not_with_and_or(self) -> None:
        """Test NOT combined with AND/OR operators."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # NOT ((environment = development) OR (status = error))
        # = Servers that are NOT (development OR error status)
        not_condition = ServerWhereInput()
        not_condition.OR = [
            ServerWhereInput(environment={"equals": "development"}),
            ServerWhereInput(status={"equals": "error"}),
        ]

        where_filter = ServerWhereInput()
        where_filter.NOT = not_condition

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should exclude: dev-test-01, dev-build-01 (development) + any error status
        # Should find: all production + staging servers
        assert len(result) == 5
        hostnames = {server.hostname for server in result}
        assert hostnames == {
            "prod-web-01",
            "prod-web-02",
            "prod-db-01",  # Production
            "staging-web-01",
            "staging-db-01",  # Staging
        }

    @pytest.mark.asyncio
    async def test_deeply_nested_logical_operators(self) -> None:
        """Test deeply nested logical operator combinations."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Complex query:
        # (production AND (>=8 cores OR >=32GB memory)) OR (staging AND active AND virtual)

        # Production with high specs
        prod_high_spec = ServerWhereInput()
        prod_high_spec.AND = [
            ServerWhereInput(environment={"equals": "production"}),
            ServerWhereInput(
                OR=[ServerWhereInput(cpu_cores={"gte": 8}), ServerWhereInput(memory_gb={"gte": 32})]
            ),
        ]

        # Staging active virtual
        staging_active_virtual = ServerWhereInput()
        staging_active_virtual.AND = [
            ServerWhereInput(environment={"equals": "staging"}),
            ServerWhereInput(status={"equals": "active"}),
            ServerWhereInput(is_virtual={"equals": True}),
        ]

        where_filter = ServerWhereInput()
        where_filter.OR = [prod_high_spec, staging_active_virtual]

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find:
        # - Production high-spec: prod-web-01, prod-web-02, prod-db-01
        # - Staging active virtual: staging-web-01
        assert len(result) == 4
        hostnames = {server.hostname for server in result}
        assert hostnames == {
            "prod-web-01",
            "prod-web-02",
            "prod-db-01",  # Production high-spec
            "staging-web-01",  # Staging active virtual
        }

    @pytest.mark.asyncio
    async def test_multiple_field_operators_with_logical_operators(self) -> None:
        """Test combining field operators (contains, gte, etc.) with logical operators."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Complex field + logical operator combination:
        # (hostname contains "web" AND memory >= 16) OR (hostname contains "db" AND cpu >= 8)

        web_condition = ServerWhereInput()
        web_condition.AND = [
            ServerWhereInput(hostname={"contains": "web"}),
            ServerWhereInput(memory_gb={"gte": 16}),
        ]

        db_condition = ServerWhereInput()
        db_condition.AND = [
            ServerWhereInput(hostname={"contains": "db"}),
            ServerWhereInput(cpu_cores={"gte": 8}),
        ]

        where_filter = ServerWhereInput()
        where_filter.OR = [web_condition, db_condition]

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find:
        # - Web servers with >=16GB: prod-web-01, prod-web-02, staging-web-01
        # - DB servers with >=8 cores: prod-db-01
        assert len(result) == 4
        hostnames = {server.hostname for server in result}
        assert hostnames == {
            "prod-web-01",
            "prod-web-02",
            "staging-web-01",  # Web with >=16GB
            "prod-db-01",  # DB with >=8 cores
        }

    @pytest.mark.asyncio
    async def test_empty_logical_operator_arrays(self) -> None:
        """Test behavior with empty AND/OR arrays."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Empty AND array should return all items
        where_filter = ServerWhereInput()
        where_filter.AND = []

        result = await resolver(self.test_datacenter, None, where=where_filter)
        assert len(result) == 7  # All servers

        # Empty OR array should return no items
        where_filter = ServerWhereInput()
        where_filter.OR = []

        result = await resolver(self.test_datacenter, None, where=where_filter)
        assert len(result) == 0  # No servers

    @pytest.mark.asyncio
    async def test_logical_operators_with_null_values(self) -> None:
        """Test logical operators with null value filtering."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # (has IP address) OR (environment = development)
        has_ip_condition = ServerWhereInput()
        has_ip_condition.ip_address = {"isnull": False}

        dev_condition = ServerWhereInput()
        dev_condition.environment = {"equals": "development"}

        where_filter = ServerWhereInput()
        where_filter.OR = [has_ip_condition, dev_condition]

        result = await resolver(self.test_datacenter, None, where=where_filter)

        # Should find: all servers with IPs OR all development servers
        # staging-db-01 has no IP and is not development, so excluded
        assert len(result) == 6  # All servers except staging-db-01
        hostnames = {server.hostname for server in result}
        expected_hostnames = {
            "prod-web-01",
            "prod-web-02",
            "prod-db-01",  # Production with IPs
            "staging-web-01",  # Staging with IP
            "dev-test-01",
            "dev-build-01",  # Development (both have IPs and are development)
        }
        assert hostnames == expected_hostnames

    @pytest.mark.asyncio
    async def test_performance_with_complex_nested_conditions(self) -> None:
        """Test that complex nested conditions don't cause performance issues."""
        ServerWhereInput = create_graphql_where_input(Server)
        resolver = create_nested_array_field_resolver_with_where("servers", list[Server])

        # Very complex nested condition for performance testing
        complex_condition = ServerWhereInput()
        complex_condition.OR = [
            ServerWhereInput(
                AND=[
                    ServerWhereInput(environment={"in": ["production", "staging"]}),
                    ServerWhereInput(
                        OR=[
                            ServerWhereInput(cpu_cores={"gte": 4}),
                            ServerWhereInput(memory_gb={"gte": 8}),
                        ]
                    ),
                    ServerWhereInput(NOT=ServerWhereInput(status={"equals": "error"})),
                ]
            ),
            ServerWhereInput(
                AND=[
                    ServerWhereInput(hostname={"contains": "dev"}),
                    ServerWhereInput(is_virtual={"equals": True}),
                    ServerWhereInput(ip_address={"isnull": False}),
                ]
            ),
        ]

        import time

        start_time = time.time()
        result = await resolver(self.test_datacenter, None, where=complex_condition)
        end_time = time.time()

        # Should complete quickly (< 0.1 seconds for 7 items)
        assert (end_time - start_time) < 0.1

        # Verify correct filtering logic was applied
        assert len(result) >= 1  # Should find some matches

        # All results should match at least one of the OR conditions
        for server in result:
            matches_first_condition = (
                server.environment in ["production", "staging"]
                and (server.cpu_cores >= 4 or server.memory_gb >= 8)
                and server.status != "error"
            )
            matches_second_condition = (
                "dev" in server.hostname and server.is_virtual and server.ip_address is not None
            )
            assert matches_first_condition or matches_second_condition
