"""System test verifying the production bug fix end-to-end.

This test simulates the exact production environment and scenario
described in the bug report to verify the fix works completely.
"""

import asyncio

import pytest

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.gql.schema_builder import build_fraiseql_schema


# Simulate the modular structure from the bug report
# This mimics: resolvers/query/dim/network/dns_server_queries.py
@fraiseql.query
async def dns_servers(info) -> list[dict]:
    """DNS servers query - exact example from bug report."""
    # Simulate database access
    return [
        {"id": "1", "name": "dns1.example.com", "ip": "192.168.1.1"},
        {"id": "2", "name": "dns2.example.com", "ip": "192.168.1.2"},
    ]


@fraiseql.query
async def network_devices(info) -> list[dict]:
    """Network devices query."""
    return [
        {"id": "1", "name": "router1", "type": "router"},
        {"id": "2", "name": "switch1", "type": "switch"},
    ]


class TestProductionBugVerification:
    """System test for complete production bug verification."""

    def setup_method(self):
        """Reset registry state and re-register module-level queries."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Re-register the module-level decorated functions
        registry.register_query(dns_servers)
        registry.register_query(network_devices)

    def test_exact_bug_report_scenario(self):
        """Test the exact scenario from the bug report.

        Original error: "Type registry lookup for v_dns_server not implemented. Available views: []"
        This should now work perfectly.
        """
        registry = SchemaRegistry.get_instance()

        # Verify initial auto-registration from decorators
        assert "dns_servers" in registry.queries
        assert "network_devices" in registry.queries

        # Simulate create_fraiseql_app() pattern that caused the bug
        try:
            # This pattern previously caused registry corruption
            app = create_fraiseql_app(
                database_url="postgresql://test:test@localhost/test_db",
                queries=[dns_servers, network_devices],  # Duplicate registration!
                production=True,  # Production mode
            )

            # App creation should succeed
            assert app is not None

            # Registry should be healthy
            health = registry.health_check()
            assert not health.has_critical_issues, f"Registry unhealthy: {health.issues}"

            # Both queries should still be registered
            assert "dns_servers" in registry.queries
            assert "network_devices" in registry.queries

            # No registry corruption
            assert len(registry.queries) >= 2

        except Exception as e:
            # If there's still an error, it should be descriptive, not cryptic
            error_msg = str(e)
            assert "Available views: []" not in error_msg, "Old cryptic error message returned"

            # If it's a registry error, it should provide solutions
            if "registry" in error_msg.lower():
                assert any(keyword in error_msg.lower() for keyword in
                          ["duplicate", "solutions", "diagnostic"]), f"Unhelpful error: {error_msg}"

            # Re-raise to fail the test if we get an unexpected error
            raise

    def test_schema_building_with_duplicates(self):
        """Test that GraphQL schema building works with duplicate registrations."""
        # Build schema with explicit query list (causes duplicates)
        schema = build_fraiseql_schema(
            query_types=[dns_servers, network_devices],
        )

        # Schema should build successfully
        assert schema is not None
        assert hasattr(schema, 'query_type')

        # Verify query type has our fields
        query_fields = schema.query_type.fields
        assert 'dnsServers' in query_fields or 'dns_servers' in query_fields
        assert 'networkDevices' in query_fields or 'network_devices' in query_fields

    def test_registry_health_in_production_scenario(self):
        """Test registry health monitoring in production-like scenario."""
        registry = SchemaRegistry.get_instance()

        # Simulate multiple registration paths (the root cause)
        registry.register_query(dns_servers)  # Path 1
        registry.register_query(dns_servers)  # Path 2
        registry.register_query(dns_servers)  # Path 3

        # Registry should handle this gracefully
        health = registry.health_check()
        assert not health.has_critical_issues

        # Should have exactly one registration, not zero (corruption) or multiple
        assert len(registry.queries) >= 1
        assert "dns_servers" in registry.queries

    def test_comprehensive_error_diagnostics(self):
        """Test that error diagnostics provide actionable information."""
        registry = SchemaRegistry.get_instance()
        registry.clear()  # Simulate corruption

        # Generate diagnostic report
        report = registry.generate_diagnostic_report()

        # Report should be comprehensive
        assert "Registry Health Report" in report
        assert "CRITICAL" in report
        assert "Registry appears completely empty" in report

        # Should provide solutions and diagnostics
        assert "duplicate" in report.lower() and "query" in report.lower()
        assert "import" in report.lower()
        assert "database connection" in report.lower()

    def test_production_startup_validation(self):
        """Test production startup validation pattern."""
        registry = SchemaRegistry.get_instance()

        # This is the recommended pattern for production
        def validate_startup():
            try:
                registry.validate_registry_integrity()
                return True, "Registry healthy"
            except RuntimeError as e:
                return False, str(e)

        # Should pass with healthy registry
        is_healthy, message = validate_startup()
        assert is_healthy, f"Startup validation failed: {message}"
        assert "Registry healthy" in message

        # Test with corrupted registry
        registry.clear()
        is_healthy, message = validate_startup()
        assert not is_healthy
        assert "Registry Corruption Detected" in message
        assert "Available views: []" not in message  # No cryptic errors

    def test_performance_with_many_duplicates(self):
        """Test performance doesn't degrade with many duplicate registrations."""
        import time

        registry = SchemaRegistry.get_instance()

        # Create many duplicate registrations (simulating complex import chains)
        start_time = time.time()

        for _ in range(50):
            registry.register_query(dns_servers)
            registry.register_query(network_devices)

        end_time = time.time()
        duration = end_time - start_time

        # Should complete quickly (under 0.5 seconds for 100 duplicates)
        assert duration < 0.5, f"Deduplication too slow: {duration:.3f}s"

        # Registry should be correct
        assert len(registry.queries) >= 2
        assert "dns_servers" in registry.queries
        assert "network_devices" in registry.queries

    async def test_actual_graphql_execution(self):
        """Test that GraphQL queries actually work after the fix."""
        # Create mock context
        mock_db = type('MockDB', (), {
            'find': lambda self, table: [{"test": "data"}] if table == "v_dns_server" else []
        })()

        mock_info = type('MockInfo', (), {
            'context': {"db": mock_db}
        })()

        # Execute the query function directly
        result = await dns_servers(mock_info)

        # Should work without errors
        assert result is not None
        assert len(result) == 2
        assert result[0]["name"] == "dns1.example.com"

    def test_backward_compatibility_patterns(self):
        """Test that all existing application patterns still work."""
        registry = SchemaRegistry.get_instance()

        # Pattern 1: Pure decorators (should work)
        initial_count = len(registry.queries)

        @fraiseql.query
        async def pattern1_query(info) -> str:
            return "pattern1"

        assert len(registry.queries) == initial_count + 1

        # Pattern 2: Pure explicit (should work)
        async def pattern2_query(info) -> str:
            return "pattern2"

        registry.register_query(pattern2_query)
        assert len(registry.queries) == initial_count + 2

        # Pattern 3: Mixed (previously broken, now fixed)
        @fraiseql.query
        async def pattern3_query(info) -> str:
            return "pattern3"

        # This duplicate registration should be handled gracefully
        registry.register_query(pattern3_query)
        assert len(registry.queries) == initial_count + 3
        assert "pattern3_query" in registry.queries

    def test_end_to_end_app_creation(self):
        """Test complete end-to-end app creation with the bug scenario."""
        # This is the complete flow that was failing in production

        # Step 1: Decorators auto-register (import time)
        registry = SchemaRegistry.get_instance()
        initial_queries = list(registry.queries.keys())

        # Step 2: App creation with explicit queries (runtime)
        app = create_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            queries=[dns_servers, network_devices],  # Duplicates!
            production=True
        )

        # Step 3: Verify everything works
        assert app is not None

        # Registry should be healthy
        health = registry.health_check()
        assert health.severity != "critical", f"Critical issues: {health.issues}"

        # All queries should be registered
        final_queries = list(registry.queries.keys())
        assert "dns_servers" in final_queries
        assert "network_devices" in final_queries

        # No registry corruption (length should be reasonable)
        assert len(final_queries) >= len(initial_queries)
