"""Comprehensive tests for entity-aware query routing system."""

import pytest
from graphql import build_schema

from fraiseql.execution.mode_selector import ExecutionMode, ModeSelector
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.routing.config import EntityRoutingConfig
from fraiseql.routing.entity_extractor import EntityExtractor
from fraiseql.routing.query_router import QueryRouter


class TestEntityRoutingSystem:
    """Test the complete entity routing system."""

    @pytest.fixture
    def test_schema(self):
        """Create a test GraphQL schema."""
        schema_def = """
            type Query {
                allocations: [Allocation]
                contracts: [Contract]
                dnsServers: [DnsServer]
                gateways: [Gateway]
            }

            type Allocation {
                id: ID!
                machine: Machine
                contract: Contract
            }

            type Contract {
                id: ID!
                number: String
            }

            type Machine {
                id: ID!
                name: String
            }

            type DnsServer {
                id: ID!
                ipAddress: String
                identifier: String
            }

            type Gateway {
                id: ID!
                ipAddress: String
                name: String
            }
        """
        return build_schema(schema_def)

    @pytest.fixture
    def routing_config(self):
        """Create entity routing configuration."""
        return EntityRoutingConfig(
            turbo_entities=["allocation", "contract", "machine"],
            normal_entities=["dns_server", "gateway"],
            mixed_query_strategy="normal",
            auto_routing_enabled=True,
        )

    @pytest.fixture
    def fraiseql_config_with_routing(self, routing_config):
        """Create FraiseQLConfig with entity routing."""
        return FraiseQLConfig(
            database_url="postgresql://user:pass@localhost/test",
            entity_routing=routing_config,
        )

    def test_entity_routing_config_creation(self, routing_config):
        """Test EntityRoutingConfig can be created and validated."""
        assert routing_config.turbo_entities == ["allocation", "contract", "machine"]
        assert routing_config.normal_entities == ["dns_server", "gateway"]
        assert routing_config.mixed_query_strategy == "normal"
        assert routing_config.auto_routing_enabled is True

    def test_entity_routing_config_validation(self):
        """Test EntityRoutingConfig validates overlapping entities."""
        with pytest.raises(ValueError, match="Entities cannot be in both turbo and normal lists"):
            EntityRoutingConfig(
                turbo_entities=["shared_entity"],
                normal_entities=["shared_entity"],
            )

    def test_fraiseql_config_integration(self, routing_config):
        """Test FraiseQLConfig integrates with entity routing."""
        config = FraiseQLConfig(
            database_url="postgresql://user:pass@localhost/test",
            entity_routing=routing_config,
        )

        assert config.entity_routing is not None
        assert isinstance(config.entity_routing, EntityRoutingConfig)
        assert config.entity_routing.turbo_entities == ["allocation", "contract", "machine"]

    def test_entity_extractor(self, test_schema):
        """Test EntityExtractor can analyze GraphQL queries."""
        extractor = EntityExtractor(test_schema)

        query = """
            query GetAllocations {
                allocations {
                    id
                    machine { id name }
                    contract { id number }
                }
            }
        """

        result = extractor.extract_entities(query)

        assert "allocation" in result.entities
        assert "machine" in result.entities
        assert "contract" in result.entities
        assert result.root_entities == ["allocation"]
        assert set(result.nested_entities) == {"machine", "contract"}
        assert result.operation_type == "query"
        assert len(result.analysis_errors) == 0

    def test_query_router_turbo_entities(self, test_schema, routing_config):
        """Test QueryRouter routes turbo entities to TURBO mode."""
        extractor = EntityExtractor(test_schema)
        router = QueryRouter(routing_config, extractor)

        query = """
            query GetAllocations {
                allocations {
                    id
                    machine { id name }
                    contract { id number }
                }
            }
        """

        mode = router.determine_execution_mode(query)
        assert mode == ExecutionMode.TURBO

    def test_query_router_normal_entities(self, test_schema, routing_config):
        """Test QueryRouter routes normal entities to NORMAL mode."""
        extractor = EntityExtractor(test_schema)
        router = QueryRouter(routing_config, extractor)

        query = """
            query GetDnsServers {
                dnsServers {
                    id
                    ipAddress
                    identifier
                }
            }
        """

        mode = router.determine_execution_mode(query)
        assert mode == ExecutionMode.NORMAL

    def test_query_router_mixed_entities(self, test_schema, routing_config):
        """Test QueryRouter handles mixed entities with strategy."""
        extractor = EntityExtractor(test_schema)
        router = QueryRouter(routing_config, extractor)

        query = """
            query GetMixed {
                allocations { id }
                dnsServers { id ipAddress }
            }
        """

        mode = router.determine_execution_mode(query)
        assert mode == ExecutionMode.NORMAL

    def test_mode_selector_integration(self, test_schema, fraiseql_config_with_routing):
        """Test ModeSelector integrates with entity routing."""
        mode_selector = ModeSelector(fraiseql_config_with_routing)

        extractor = EntityExtractor(test_schema)
        router = QueryRouter(fraiseql_config_with_routing.entity_routing, extractor)
        mode_selector.set_query_router(router)

        query = """
            query GetAllocations {
                allocations {
                    id
                    machine { id name }
                }
            }
        """

        mode = mode_selector.select_mode(query, {}, {})
        assert mode == ExecutionMode.TURBO

    def test_mode_hint_overrides_entity_routing(self, test_schema, fraiseql_config_with_routing):
        """Test that mode hints take precedence over entity routing."""
        mode_selector = ModeSelector(fraiseql_config_with_routing)

        extractor = EntityExtractor(test_schema)
        router = QueryRouter(fraiseql_config_with_routing.entity_routing, extractor)
        mode_selector.set_query_router(router)

        query = """
            # @mode: normal
            query GetAllocations {
                allocations {
                    id
                    machine { id name }
                }
            }
        """

        mode = mode_selector.select_mode(query, {}, {})
        assert mode == ExecutionMode.NORMAL

    def test_disabled_entity_routing(self, test_schema):
        """Test behavior when entity routing is disabled."""
        config = FraiseQLConfig(
            database_url="postgresql://user:pass@localhost/test",
            entity_routing=EntityRoutingConfig(
                turbo_entities=["allocation"],
                normal_entities=["dns_server"],
                auto_routing_enabled=False,
            ),
        )

        mode_selector = ModeSelector(config)
        extractor = EntityExtractor(test_schema)
        router = QueryRouter(config.entity_routing, extractor)
        mode_selector.set_query_router(router)

        query = """
            query GetAllocations {
                allocations { id }
            }
        """

        mode = mode_selector.select_mode(query, {}, {})
        assert mode == ExecutionMode.NORMAL

    def test_routing_metrics(self, test_schema, routing_config):
        """Test QueryRouter provides useful metrics."""
        extractor = EntityExtractor(test_schema)
        router = QueryRouter(routing_config, extractor)

        metrics = router.get_routing_metrics()

        assert metrics["auto_routing_enabled"] is True
        assert metrics["turbo_entities_count"] == 3
        assert metrics["normal_entities_count"] == 2
        assert metrics["mixed_query_strategy"] == "normal"
        assert "allocation" in metrics["turbo_entities"]
        assert "dns_server" in metrics["normal_entities"]
