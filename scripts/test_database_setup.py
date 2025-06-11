#!/usr/bin/env python3
"""Test script to verify database testing setup is working correctly."""

import asyncio
import os
import sys
from pathlib import Path

# Add src to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

try:
    import psycopg
    import psycopg_pool
    from testcontainers.postgres import PostgresContainer
except ImportError as e:
    print(f"Error: Missing required dependencies: {e}")
    print("\nPlease install test dependencies:")
    print("  pip install -e '.[dev]'")
    sys.exit(1)


async def test_testcontainers():
    """Test that testcontainers work properly."""
    print("1. Testing testcontainers setup...")

    try:
        # Check if we should use Podman
        use_podman = os.environ.get("TESTCONTAINERS_PODMAN", "false").lower() == "true"
        print(f"   Using {'Podman' if use_podman else 'Docker'}...")

        with PostgresContainer(
            image="postgres:16-alpine", user="test", password="test", dbname="test_db"
        ) as postgres:
            print("   ✓ Container started successfully")

            # Get connection URL
            url = postgres.get_connection_url()
            print(f"   ✓ Connection URL: {url}")

            # Test connection
            async with await psycopg.AsyncConnection.connect(url) as conn:
                async with conn.cursor() as cur:
                    await cur.execute("SELECT version()")
                    version = await cur.fetchone()
                    print(f"   ✓ Connected to PostgreSQL: {version[0][:30]}...")

                    # Test JSONB support
                    await cur.execute("SELECT '{\"test\": true}'::jsonb")
                    result = await cur.fetchone()
                    assert result[0] == {"test": True}
                    print("   ✓ JSONB support verified")

        print("   ✓ Container cleaned up successfully\n")
        return True

    except Exception as e:
        print(f"   ✗ Error: {e}\n")
        return False


async def test_connection_pool():
    """Test connection pool setup."""
    print("2. Testing connection pool...")

    try:
        # Start a container for this test
        postgres = PostgresContainer(
            image="postgres:16-alpine",
            user="pool_test",
            password="pool_test",
            dbname="pool_db",
        )
        postgres.start()

        try:
            url = postgres.get_connection_url()

            # Create pool
            async with psycopg_pool.AsyncConnectionPool(
                url, min_size=2, max_size=5
            ) as pool:
                print("   ✓ Connection pool created")

                # Test concurrent connections
                async def run_query(query_id):
                    async with pool.connection() as conn:
                        async with conn.cursor() as cur:
                            await cur.execute(
                                "SELECT pg_backend_pid(), %s", (query_id,)
                            )
                            return await cur.fetchone()

                results = await asyncio.gather(run_query(1), run_query(2), run_query(3))

                print(f"   ✓ Executed {len(results)} concurrent queries")
                pids = [r[0] for r in results]
                print(f"   ✓ Used PIDs: {pids}")

        finally:
            postgres.stop()

        print("   ✓ Pool and container cleaned up\n")
        return True

    except Exception as e:
        print(f"   ✗ Error: {e}\n")
        return False


async def test_fraiseql_repository():
    """Test FraiseQL repository with real database."""
    print("3. Testing FraiseQL repository...")

    try:
        from psycopg.sql import SQL

        from fraiseql.db import DatabaseQuery, FraiseQLRepository

        # Start container
        postgres = PostgresContainer(
            image="postgres:16-alpine",
            user="fraise_test",
            password="fraise_test",
            dbname="fraise_db",
        )
        postgres.start()

        try:
            url = postgres.get_connection_url()

            # Create pool and repository
            async with psycopg_pool.AsyncConnectionPool(url, min_size=1) as pool:
                repo = FraiseQLRepository(pool=pool)
                print("   ✓ Repository created")

                # Create test table
                await repo.run(
                    DatabaseQuery(
                        statement=SQL("""
                        CREATE TABLE test_users (
                            id SERIAL PRIMARY KEY,
                            data JSONB NOT NULL DEFAULT '{}'::jsonb
                        )
                    """),
                        params={},
                        fetch_result=False,
                    )
                )
                print("   ✓ Created test table")

                # Insert test data
                result = await repo.run(
                    DatabaseQuery(
                        statement=SQL("""
                        INSERT INTO test_users (data)
                        VALUES (%(data)s::jsonb)
                        RETURNING id, data->>'name' as name
                    """),
                        params={"data": '{"name": "Test User", "active": true}'},
                        fetch_result=True,
                    )
                )
                print(f"   ✓ Inserted test data: {result[0]}")

                # Query with JSONB
                result = await repo.run(
                    DatabaseQuery(
                        statement=SQL("""
                        SELECT data->>'name' as name
                        FROM test_users
                        WHERE data @> '{"active": true}'::jsonb
                    """),
                        params={},
                        fetch_result=True,
                    )
                )
                print(f"   ✓ JSONB query successful: {result[0]}")

        finally:
            postgres.stop()

        print("   ✓ Repository test completed\n")
        return True

    except Exception as e:
        print(f"   ✗ Error: {e}\n")
        import traceback

        traceback.print_exc()
        return False


async def main():
    """Run all tests."""
    print("=== FraiseQL Database Testing Setup Verification ===\n")

    # Check environment
    print("Environment:")
    print(f"  Python: {sys.version.split()[0]}")
    print(f"  Podman mode: {os.environ.get('TESTCONTAINERS_PODMAN', 'false')}")
    print(f"  Working directory: {os.getcwd()}\n")

    # Run tests
    results = []
    results.append(await test_testcontainers())
    results.append(await test_connection_pool())
    results.append(await test_fraiseql_repository())

    # Summary
    print("=== Summary ===")
    if all(results):
        print("✅ All tests passed! Database testing is properly configured.")
        print("\nYou can now run the test suite with:")
        print("  pytest                    # Run all tests")
        print("  pytest -m database        # Run only database tests")
        print("  pytest --no-db           # Skip database tests")
        print("\nFor Podman users:")
        print("  TESTCONTAINERS_PODMAN=true pytest")
        return 0
    else:
        print("❌ Some tests failed. Please check the errors above.")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
