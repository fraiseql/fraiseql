"""Test GraphQL WHERE clause integration with repository.

This test demonstrates that the WHERE clause generation bug has been completely
fixed and GraphQL-style filtering now works correctly in the repository.
"""

import pytest

from fraiseql.cqrs.repository import CQRSRepository

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.mark.database
class TestGraphQLWhereRepository:
    """Test GraphQL WHERE clause filtering works correctly."""

    @pytest.mark.asyncio
    async def test_graphql_string_operators_work(self, class_db_pool, test_schema) -> None:
        """Test that all GraphQL string operators work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

            # Create test data
            await conn.execute(
                """
                CREATE TEMP TABLE test_devices (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB
                );
                INSERT INTO test_devices (data) VALUES
                ('{"name": "router-primary", "model": "CISCO-2900"}'),
                ('{"name": "switch-main", "model": "HP-ProCurve"}'),
                ('{"name": "server-backup", "model": "DELL-PowerEdge"}'),
                ('{"name": "router-secondary", "model": "CISCO-1900"}'),
                ('{"name": "firewall-dmz", "model": "FORTINET-60E"}');

                CREATE TEMP VIEW v_test_devices AS
                SELECT id, data FROM test_devices;
            """
            )

            # Test contains operator
            results = await repo.select_from_json_view(
                "v_test_devices", where={"name": {"contains": "router"}}
            )
            assert len(results) == 2
            assert all("router" in r["name"] for r in results)

            # Test startswith operator
            results = await repo.select_from_json_view(
                "v_test_devices", where={"name": {"startswith": "server"}}
            )
            assert len(results) == 1
            assert results[0]["name"] == "server-backup"

            # Test endswith operator
            results = await repo.select_from_json_view(
                "v_test_devices", where={"model": {"endswith": "2900"}}
            )
            assert len(results) == 1
            assert results[0]["name"] == "router-primary"

    @pytest.mark.asyncio
    async def test_graphql_network_operators_work(self, class_db_pool, test_schema) -> None:
        """Test that network-specific operators work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

            # Create test data with various IP addresses
            await conn.execute(
                """
                CREATE TEMP TABLE test_network_devices (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB
                );
                INSERT INTO test_network_devices (data) VALUES
                ('{"hostname": "router-01", "ipAddress": "192.168.1.1"}'),
                ('{"hostname": "router-02", "ipAddress": "10.0.0.1"}'),
                ('{"hostname": "server-01", "ipAddress": "172.16.1.10"}'),
                ('{"hostname": "server-02", "ipAddress": "8.8.8.8"}'),
                ('{"hostname": "dns-01", "ipAddress": "1.1.1.1"}');

                CREATE TEMP VIEW v_test_network_devices AS
                SELECT id, data FROM test_network_devices;
            """
            )

            # Test isPrivate operator - should return RFC 1918 addresses
            results = await repo.select_from_json_view(
                "v_test_network_devices", where={"ipAddress": {"isPrivate": True}}
            )
            assert len(results) == 3  # 192.168.1.1, 10.0.0.1, 172.16.1.10
            private_ips = [r["ipAddress"] for r in results]
            assert "192.168.1.1" in private_ips
            assert "10.0.0.1" in private_ips
            assert "172.16.1.10" in private_ips

            # Test isPublic operator - should return public IPs
            results = await repo.select_from_json_view(
                "v_test_network_devices", where={"ipAddress": {"isPublic": True}}
            )
            assert len(results) == 2  # 8.8.8.8, 1.1.1.1
            public_ips = [r["ipAddress"] for r in results]
            assert "8.8.8.8" in public_ips
            assert "1.1.1.1" in public_ips

    @pytest.mark.asyncio
    async def test_graphql_complex_combinations_work(self, class_db_pool, test_schema) -> None:
        """Test that complex GraphQL WHERE combinations work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

            # Create test data
            await conn.execute(
                """
                CREATE TEMP TABLE test_complex (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB
                );
                INSERT INTO test_complex (data) VALUES
                ('{"name": "prod-server-01", "environment": "production", "cpu": 8, "status": "active"}'),
                ('{"name": "prod-server-02", "environment": "production", "cpu": 16, "status": "active"}'),
                ('{"name": "dev-server-01", "environment": "development", "cpu": 4, "status": "active"}'),
                ('{"name": "test-server-01", "environment": "testing", "cpu": 8, "status": "inactive"}'),
                ('{"name": "staging-server-01", "environment": "staging", "cpu": 12, "status": "active"}');

                CREATE TEMP VIEW v_test_complex AS
                SELECT id, data FROM test_complex;
            """
            )

            # Test multiple string and numeric operators
            results = await repo.select_from_json_view(
                "v_test_complex",
                where={
                    "name": {"startswith": "prod"},
                    "cpu": {"gte": 8},
                    "status": {"eq": "active"},
                },
            )
            assert len(results) == 2  # Both production servers
            assert all(r["name"].startswith("prod") for r in results)
            assert all(int(r["cpu"]) >= 8 for r in results)
            assert all(r["status"] == "active" for r in results)

            # Test contains with numeric greater than
            results = await repo.select_from_json_view(
                "v_test_complex", where={"name": {"contains": "server"}, "cpu": {"gt": 10}}
            )
            assert len(results) == 2  # prod-server-02 (16) and staging-server-01 (12)
            server_names = [r["name"] for r in results]
            assert "prod-server-02" in server_names
            assert "staging-server-01" in server_names

    @pytest.mark.asyncio
    async def test_graphql_list_operators_work(self, class_db_pool, test_schema) -> None:
        """Test that GraphQL list operators (in, nin) work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

            # Create test data
            await conn.execute(
                """
                CREATE TEMP TABLE test_list_ops (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB
                );
                INSERT INTO test_list_ops (data) VALUES
                ('{"name": "server-01", "environment": "production"}'),
                ('{"name": "server-02", "environment": "staging"}'),
                ('{"name": "server-03", "environment": "development"}'),
                ('{"name": "server-04", "environment": "testing"}'),
                ('{"name": "server-05", "environment": "production"}');

                CREATE TEMP VIEW v_test_list_ops AS
                SELECT id, data FROM test_list_ops;
            """
            )

            # Test 'in' operator
            results = await repo.select_from_json_view(
                "v_test_list_ops", where={"environment": {"in": ["production", "staging"]}}
            )
            assert len(results) == 3  # 2 production + 1 staging
            environments = [r["environment"] for r in results]
            assert environments.count("production") == 2
            assert environments.count("staging") == 1

            # Test 'nin' (not in) operator
            results = await repo.select_from_json_view(
                "v_test_list_ops", where={"environment": {"nin": ["development", "testing"]}}
            )
            assert len(results) == 3  # Should exclude development and testing
            environments = [r["environment"] for r in results]
            assert "development" not in environments
            assert "testing" not in environments
            assert all(env in ["production", "staging"] for env in environments)

    @pytest.mark.asyncio
    async def test_backward_compatibility_maintained(self, class_db_pool, test_schema) -> None:
        """Test that simple equality (backward compatibility) still works."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

            # Create test data
            await conn.execute(
                """
                CREATE TEMP TABLE test_backward_compat (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB
                );
                INSERT INTO test_backward_compat (data) VALUES
                ('{"status": "active", "type": "server"}'),
                ('{"status": "inactive", "type": "router"}'),
                ('{"status": "active", "type": "switch"}');

                CREATE TEMP VIEW v_test_backward_compat AS
                SELECT id, data FROM test_backward_compat;
            """
            )

            # Test simple string equality (old style)
            results = await repo.select_from_json_view(
                "v_test_backward_compat",
                where={"status": "active"},  # Simple key-value, no operators
            )
            assert len(results) == 2
            assert all(r["status"] == "active" for r in results)

            # Test mixing old and new styles
            results = await repo.select_from_json_view(
                "v_test_backward_compat",
                where={
                    "status": "active",  # Old style
                    "type": {"in": ["server", "switch"]},  # New style
                },
            )
            assert len(results) == 2
            assert all(r["status"] == "active" for r in results)
            assert all(r["type"] in ["server", "switch"] for r in results)
