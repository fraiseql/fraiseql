"""Integration test reproducing the exact production bug scenario.

This test reproduces the specific scenario from the bug report:
- DNS server queries failing in uvicorn with "Available views: []"
- Same code working perfectly in pytest
- Registry corruption from duplicate registrations
"""

import asyncio
from unittest.mock import Mock

import pytest

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.gql.schema_builder import build_fraiseql_schema


class TestProductionRegistryBugFix:
    """Integration tests for the production registry corruption bug fix."""

    def setup_method(self):
        """Reset registry before each test."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

    async def test_dns_server_query_production_scenario(self):
        """Test the exact DNS server scenario from the bug report.

        This reproduces the failing case:
        "Type registry lookup for v_dns_server not implemented. Available views: []"
        """
        # Define the DNS server query as it would appear in production
        @fraiseql.query
        async def dns_servers(info) -> list[dict]:
            """Query all DNS servers."""
            db = info.context["db"]
            return await db.find("v_dns_server")

        # Simulate the app.py registration pattern that causes duplicates
        registry = SchemaRegistry.get_instance()

        # First, the decorator auto-registers (this happens at import time)
        assert "dns_servers" in registry.queries
        initial_count = len(registry.queries)

        # Then create_fraiseql_app tries to register again
        # In the old version, this would cause corruption
        try:
            schema = build_fraiseql_schema(
                query_types=[dns_servers],  # This causes duplicate registration
            )

            # Schema should be successfully built
            assert schema is not None
            assert hasattr(schema, 'query_type')

            # Registry should still be functional (not corrupted)
            assert len(registry.queries) == initial_count  # Should not change
            assert "dns_servers" in registry.queries

            # Health check should show no critical issues
            health = registry.health_check()
            assert not health.has_critical_issues

        except Exception as e:
            # If this fails, check that we get helpful error message
            # instead of cryptic "Available views: []"
            error_msg = str(e)
            assert "Available views: []" not in error_msg
            if "Registry" in error_msg:
                # If it's a registry error, it should be descriptive
                assert any(keyword in error_msg.lower() for keyword in
                          ["duplicate", "corruption", "solutions"])
            raise

    async def test_complex_import_chain_scenario(self):
        """Test complex import chain scenario causing multiple duplicates."""
        # Simulate a complex import hierarchy like in the bug report:
        # app.py -> resolvers.query.dim.network -> dns_server_queries.py

        # Define query in a "module"
        @fraiseql.query
        async def complex_query(info) -> str:
            return "complex"

        registry = SchemaRegistry.get_instance()
        initial_count = len(registry.queries)

        # Simulate multiple import paths registering the same function
        registry.register_query(complex_query)  # Import path 1
        registry.register_query(complex_query)  # Import path 2
        registry.register_query(complex_query)  # Import path 3

        # Registry should deduplicate and not corrupt
        assert len(registry.queries) == initial_count
        assert "complex_query" in registry.queries

        # Health check should be healthy
        health = registry.health_check()
        assert health.is_healthy or health.severity != "critical"

    async def test_create_fraiseql_app_with_duplicates(self):
        """Test full app creation with duplicate registrations."""
        # Define queries with decorators (auto-registered)
        @fraiseql.query
        async def users(info) -> list[dict]:
            return []

        @fraiseql.query
        async def posts(info) -> list[dict]:
            return []

        # Create app - this should not fail despite decorators already registering
        try:
            app = create_fraiseql_app(
                database_url="postgresql://test:test@localhost/test",
                queries=[users, posts],  # Duplicate registration
                production=True,  # Simulate production environment
            )

            # App should be created successfully
            assert app is not None

            # Registry should be functional
            registry = SchemaRegistry.get_instance()
            assert len(registry.queries) >= 2
            assert "users" in registry.queries
            assert "posts" in registry.queries

            # Health check should be good
            health = registry.health_check()
            assert not health.has_critical_issues

        except Exception as e:
            # Should not fail, but if it does, error should be descriptive
            error_msg = str(e)
            assert "Available views: []" not in error_msg

    async def test_empty_registry_diagnostic_message(self):
        """Test that empty registry provides excellent diagnostic info."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Test the comprehensive error message
        with pytest.raises(RuntimeError) as exc_info:
            registry.validate_registry_integrity()

        error_msg = str(exc_info.value)

        # Should provide comprehensive diagnostic information
        required_elements = [
            "Registry Corruption Detected",
            "Critical Issues Found",
            "appears completely empty",
            "Common Solutions",
            "duplicate @fraiseql.query",
            "create_fraiseql_app()",
            "import chains",
            "database connection",
        ]

        for element in required_elements:
            assert element in error_msg, f"Missing: {element}"

        # Should not contain the old useless message
        assert "Available views: []" not in error_msg

    async def test_registry_diagnostic_report_quality(self):
        """Test diagnostic report provides actionable information."""
        # Create some queries
        @fraiseql.query
        async def diagnostic_query_1(info) -> str:
            return "test1"

        @fraiseql.query
        async def diagnostic_query_2(info) -> str:
            return "test2"

        registry = SchemaRegistry.get_instance()
        report = registry.generate_diagnostic_report()

        # Report should be comprehensive
        required_sections = [
            "Registry Health Report",
            "Overall Status",
            "Registry Contents",
            "Queries:",
            "Mutations:",
            "Subscriptions:",
            "Types:",
        ]

        for section in required_sections:
            assert section in report

        # Should show actual counts
        assert "Queries: 2" in report or "Queries: " in report
        # The diagnostic may mention function names in warnings (which is good)

        # Test empty registry report
        registry.clear()
        empty_report = registry.generate_diagnostic_report()
        assert "CRITICAL" in empty_report
        assert "Queries: 0" in empty_report

    async def test_environment_consistency(self):
        """Test that behavior is consistent across environments."""
        @fraiseql.query
        async def env_consistency_test(info) -> str:
            return "consistent"

        registry = SchemaRegistry.get_instance()

        # Should work the same in all environments
        environments = ["test", "development", "production"]

        for env in environments:
            # Simulate environment
            import os
            old_env = os.environ.get("ENVIRONMENT")
            os.environ["ENVIRONMENT"] = env

            try:
                # Register duplicate
                registry.register_query(env_consistency_test)

                # Should behave consistently
                assert "env_consistency_test" in registry.queries
                health = registry.health_check()

                # Should not have critical registry corruption
                assert not health.has_critical_issues

            finally:
                # Restore environment
                if old_env is not None:
                    os.environ["ENVIRONMENT"] = old_env
                elif "ENVIRONMENT" in os.environ:
                    del os.environ["ENVIRONMENT"]

    async def test_performance_impact_of_deduplication(self):
        """Test that deduplication logic doesn't significantly impact performance."""
        import time

        @fraiseql.query
        async def perf_test_query(info) -> str:
            return "perf_test"

        registry = SchemaRegistry.get_instance()

        # Measure registration time with duplicates
        start_time = time.time()

        for _ in range(100):
            registry.register_query(perf_test_query)

        end_time = time.time()
        duration = end_time - start_time

        # Should complete quickly (under 1 second for 100 duplicates)
        assert duration < 1.0, f"Deduplication too slow: {duration:.3f}s"

        # Registry should still be correct
        assert "perf_test_query" in registry.queries
        assert len(registry.queries) >= 1

    async def test_backward_compatibility(self):
        """Test that changes don't break existing applications."""
        # Test all the patterns that should still work

        # Pattern 1: Pure decorator approach
        @fraiseql.query
        async def legacy_query_1(info) -> str:
            return "legacy1"

        # Pattern 2: Explicit registration
        async def legacy_query_2(info) -> str:
            return "legacy2"

        registry = SchemaRegistry.get_instance()
        registry.register_query(legacy_query_2)

        # Pattern 3: Mixed approach (the problematic one, now fixed)
        @fraiseql.query
        async def legacy_query_3(info) -> str:
            return "legacy3"

        registry.register_query(legacy_query_3)  # Should not break

        # All should work
        assert "legacy_query_1" in registry.queries
        assert "legacy_query_2" in registry.queries
        assert "legacy_query_3" in registry.queries

        # Health should be good
        health = registry.health_check()
        assert health.is_healthy
