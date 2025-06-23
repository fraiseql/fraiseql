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
except ImportError:
    sys.exit(1)


async def test_testcontainers():
    """Test that testcontainers work properly."""
    try:
        # Check if we should use Podman
        os.environ.get("TESTCONTAINERS_PODMAN", "false").lower() == "true"

        with PostgresContainer(
            image="postgres:16-alpine",
            user="test",
            password="test",
            dbname="test_db",
        ) as postgres:
            # Get connection URL
            url = postgres.get_connection_url()

            # Test connection
            async with await psycopg.AsyncConnection.connect(url) as conn:
                async with conn.cursor() as cur:
                    await cur.execute("SELECT version()")
                    await cur.fetchone()

                    # Test JSONB support
                    await cur.execute("SELECT '{\"test\": true}'::jsonb")
                    result = await cur.fetchone()
                    assert result[0] == {"test": True}

        return True  # noqa: TRY300

    except Exception:
        return False


async def test_connection_pool():
    """Test connection pool setup."""
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
                url,
                min_size=2,
                max_size=5,
            ) as pool:
                # Test concurrent connections
                async def run_query(query_id):
                    async with pool.connection() as conn, conn.cursor() as cur:
                        await cur.execute(
                            "SELECT pg_backend_pid(), %s",
                            (query_id,),
                        )
                        return await cur.fetchone()

                results = await asyncio.gather(run_query(1), run_query(2), run_query(3))

                [r[0] for r in results]

        finally:
            postgres.stop()

        return True  # noqa: TRY300

    except Exception:
        return False


async def test_fraiseql_repository():
    """Test FraiseQL repository with real database."""
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
                    ),
                )

                # Insert test data
                await repo.run(
                    DatabaseQuery(
                        statement=SQL("""
                        INSERT INTO test_users (data)
                        VALUES (%(data)s::jsonb)
                        RETURNING id, data->>'name' as name
                    """),
                        params={"data": '{"name": "Test User", "active": true}'},
                        fetch_result=True,
                    ),
                )

                # Query with JSONB
                await repo.run(
                    DatabaseQuery(
                        statement=SQL("""
                        SELECT data->>'name' as name
                        FROM test_users
                        WHERE data @> '{"active": true}'::jsonb
                    """),
                        params={},
                        fetch_result=True,
                    ),
                )

        finally:
            postgres.stop()

        return True  # noqa: TRY300

    except Exception:
        import traceback

        traceback.print_exc()
        return False


async def main():
    """Run all tests."""
    # Check environment

    # Run tests
    results = []
    results.append(await test_testcontainers())
    results.append(await test_connection_pool())
    results.append(await test_fraiseql_repository())

    # Summary
    if all(results):
        return 0
    return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
