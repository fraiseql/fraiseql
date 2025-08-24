"""Unified database fixtures for FraiseQL testing.

This module provides a comprehensive database testing infrastructure with:
- Container-based PostgreSQL for isolated testing
- Session-scoped connection pooling for performance
- Transaction-based test isolation
- Schema management utilities
- Connection lifecycle management

The fixtures follow a layered approach:
1. Container management (session scope)
2. Connection pooling (session scope)
3. Per-test connections with transaction isolation
4. Schema utilities for complex testing scenarios
"""

import os
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Any, Dict, Optional
from uuid import uuid4

import psycopg
import pytest
import pytest_asyncio
from psycopg_pool import AsyncConnectionPool

try:
    from testcontainers.postgres import PostgresContainer

    HAS_DOCKER = True
except ImportError:
    HAS_DOCKER = False
    PostgresContainer = None

# Try to detect if Docker is actually available
if HAS_DOCKER:
    try:
        import docker

        client = docker.from_env()
        client.ping()
    except Exception:
        HAS_DOCKER = False

# Global container cache for session reuse
_container_cache: Dict[str, Any] = {}


@pytest.fixture(scope="session")
def postgres_container():
    """Session-scoped PostgreSQL container.

    Creates a single PostgreSQL container that persists for the entire
    test session, dramatically improving test performance by avoiding
    container creation overhead.

    Yields:
        PostgresContainer: The running container instance, or None if using external DB
    """
    # Check for external database configuration
    external_db_url = os.environ.get("TEST_DATABASE_URL") or os.environ.get("DATABASE_URL")
    if external_db_url:
        print(f"[fixtures.database] Using external database: {external_db_url}")
        yield None
        return

    if not HAS_DOCKER:
        pytest.skip("Docker not available for container-based testing")

    # Reuse existing container if available
    if "postgres" in _container_cache and _container_cache["postgres"].get_container_host_ip():
        print("[fixtures.database] Reusing existing PostgreSQL container")
        yield _container_cache["postgres"]
        return

    print("[fixtures.database] Starting new PostgreSQL container")
    container = PostgresContainer(
        image="postgres:16-alpine",
        username="fraiseql_test",
        password="fraiseql_test",
        dbname="fraiseql_test",
        driver="psycopg",  # Use psycopg3
        port=5432,
    )

    container.start()
    _container_cache["postgres"] = container

    yield container

    # Cleanup
    print("[fixtures.database] Stopping PostgreSQL container")
    container.stop()
    _container_cache.pop("postgres", None)


@pytest.fixture(scope="session")
def postgres_url(postgres_container) -> str:
    """Get PostgreSQL connection URL.

    Args:
        postgres_container: The container fixture

    Returns:
        str: PostgreSQL connection URL

    Raises:
        pytest.skip: If no database is available
    """
    # External database takes precedence
    external_url = os.environ.get("TEST_DATABASE_URL") or os.environ.get("DATABASE_URL")
    if external_url:
        return external_url

    # Container-based database
    if postgres_container:
        url = postgres_container.get_connection_url()
        # Normalize URL format for psycopg3
        return url.replace("postgresql+psycopg://", "postgresql://")

    pytest.skip("No PostgreSQL database available")


@pytest_asyncio.fixture(scope="session")
async def db_pool(postgres_url) -> AsyncGenerator[AsyncConnectionPool]:
    """Session-scoped database connection pool.

    Creates a connection pool that persists for the entire test session,
    enabling efficient connection reuse across all tests.

    Args:
        postgres_url: Database connection URL

    Yields:
        AsyncConnectionPool: The connection pool
    """
    print("[fixtures.database] Creating connection pool")

    pool = AsyncConnectionPool(
        postgres_url,
        min_size=2,
        max_size=10,
        timeout=30,
        open=False,  # Explicit opening to avoid deprecation warnings
    )

    await pool.open()
    await pool.wait()

    # Initialize database with required extensions
    async with pool.connection() as conn:
        await conn.execute("""
            -- Enable required extensions
            CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
            CREATE EXTENSION IF NOT EXISTS "pgcrypto";
            CREATE EXTENSION IF NOT EXISTS "ltree";
            CREATE EXTENSION IF NOT EXISTS "citext";
        """)
        await conn.commit()

    print(f"[fixtures.database] Connection pool ready (size: {pool.min_size}-{pool.max_size})")

    yield pool

    print("[fixtures.database] Closing connection pool")
    await pool.close()


@pytest_asyncio.fixture
async def db_connection(db_pool) -> AsyncGenerator[psycopg.AsyncConnection]:
    """Per-test database connection with transaction isolation.

    Each test gets its own connection within an isolated transaction
    that is automatically rolled back after the test completes,
    ensuring complete test isolation.

    Args:
        db_pool: The session connection pool

    Yields:
        AsyncConnection: Isolated database connection
    """
    async with db_pool.connection() as conn:
        # Start transaction for isolation
        await conn.execute("BEGIN")

        # Set up clean test environment
        await conn.execute("SET search_path TO public")
        await conn.execute("SET timezone TO 'UTC'")

        yield conn

        # Rollback ensures complete isolation
        await conn.execute("ROLLBACK")


@pytest_asyncio.fixture
async def db_cursor(db_connection):
    """Database cursor for simple operations.

    Args:
        db_connection: Database connection

    Yields:
        AsyncCursor: Database cursor
    """
    async with db_connection.cursor() as cursor:
        yield cursor


@pytest_asyncio.fixture
async def db_connection_committed(db_pool) -> AsyncGenerator[psycopg.AsyncConnection]:
    """Database connection with committed changes.

    Use this fixture when you need changes to persist across queries
    within the same test. Creates a unique schema for isolation and
    cleans up after the test.

    Args:
        db_pool: The session connection pool

    Yields:
        AsyncConnection: Database connection with committed changes
    """
    async with db_pool.connection() as conn:
        # Create unique test schema
        test_schema = f"test_{uuid4().hex[:8]}"

        await conn.execute(f"CREATE SCHEMA {test_schema}")
        await conn.execute(f"SET search_path TO {test_schema}, public")
        await conn.commit()

        yield conn

        # Cleanup schema
        await conn.execute(f"DROP SCHEMA {test_schema} CASCADE")
        await conn.commit()


@pytest.fixture
def db_schema_builder(db_connection):
    """Factory for creating test database schemas.

    Args:
        db_connection: Database connection

    Returns:
        Callable: Schema creation function
    """
    created_objects = []

    async def create_table(table_name: str, schema_sql: str) -> str:
        """Create a test table.

        Args:
            table_name: Name of the table
            schema_sql: CREATE TABLE SQL

        Returns:
            str: Table name
        """
        await db_connection.execute(f"DROP TABLE IF EXISTS {table_name} CASCADE")
        await db_connection.execute(schema_sql)
        created_objects.append(("table", table_name))
        return table_name

    async def create_view(view_name: str, query: str) -> str:
        """Create a test view.

        Args:
            view_name: Name of the view
            query: SELECT query for the view

        Returns:
            str: View name
        """
        await db_connection.execute(f"DROP VIEW IF EXISTS {view_name} CASCADE")
        await db_connection.execute(f"CREATE VIEW {view_name} AS {query}")
        created_objects.append(("view", view_name))
        return view_name

    async def create_function(func_name: str, func_sql: str) -> str:
        """Create a test function.

        Args:
            func_name: Name of the function
            func_sql: CREATE FUNCTION SQL

        Returns:
            str: Function name
        """
        await db_connection.execute(f"DROP FUNCTION IF EXISTS {func_name} CASCADE")
        await db_connection.execute(func_sql)
        created_objects.append(("function", func_name))
        return func_name

    # Return builder object with methods
    builder = type(
        "SchemaBuilder",
        (),
        {
            "create_table": create_table,
            "create_view": create_view,
            "create_function": create_function,
            "_created": created_objects,
        },
    )()

    return builder


@asynccontextmanager
async def temporary_schema(connection: psycopg.AsyncConnection, schema_name: Optional[str] = None):
    """Context manager for temporary schema creation.

    Args:
        connection: Database connection
        schema_name: Optional schema name (auto-generated if not provided)

    Yields:
        str: Schema name
    """
    if not schema_name:
        schema_name = f"tmp_{uuid4().hex[:8]}"

    try:
        await connection.execute(f"CREATE SCHEMA {schema_name}")
        await connection.execute(f"SET search_path TO {schema_name}, public")
        yield schema_name
    finally:
        await connection.execute(f"DROP SCHEMA {schema_name} CASCADE")
        await connection.execute("SET search_path TO public")


# Database testing markers
database_required = pytest.mark.database
slow_database = pytest.mark.slow
