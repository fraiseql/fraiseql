"""Unified container testing system for FraiseQL.

🚀 KEY FEATURE: This module implements a UNIFIED CONTAINER APPROACH where a single
PostgreSQL container runs for the entire test session, with socket-based communication
for maximum performance.

Architecture:
- ONE container per test session (not per test)
- Socket communication for better performance
- Connection pooling for efficiency
- Transaction-based test isolation

See docs/testing/unified-container-testing.md for detailed documentation.
"""

import os
from collections.abc import AsyncGenerator

import psycopg
import psycopg_pool
import pytest
import pytest_asyncio

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

# 🔑 UNIFIED CONTAINER CACHE: This is the key to our performance!
# Containers are cached and reused across test runs within the same session
_container_cache = {}


@pytest.fixture(scope="session")
def postgres_container():
    """🚀 UNIFIED CONTAINER: Single PostgreSQL instance for ALL tests.

    This is the heart of our unified container approach:
    - Started ONCE per test session (not per test)
    - Cached for test reruns
    - Communicates via socket (not HTTP)
    - Dramatically faster than per-test containers
    """
    # Skip if using external database (e.g., GitHub Actions service container)
    test_db_url = os.environ.get("TEST_DATABASE_URL")
    db_url = os.environ.get("DATABASE_URL")
    if test_db_url or db_url:
        yield None
        return

    if not HAS_DOCKER:
        pytest.skip("Docker not available")

    # Use existing container if available (for test reruns)
    if "postgres" in _container_cache and _container_cache["postgres"].get_container_host_ip():
        yield _container_cache["postgres"]
        return

    container = PostgresContainer(
        image="postgres:16-alpine",
        username="fraiseql",
        password="fraiseql",
        dbname="fraiseql_test",
        driver="psycopg",  # Use psycopg3
    )

    # Start the container
    container.start()

    # Store for reuse
    _container_cache["postgres"] = container

    yield container

    # Cleanup
    container.stop()
    _container_cache.pop("postgres", None)


@pytest.fixture(scope="session")
def postgres_url(postgres_container) -> str:
    """Get the PostgreSQL connection URL from the container or environment."""
    # Check for external database URL (e.g., GitHub Actions)
    external_url = os.environ.get("TEST_DATABASE_URL") or os.environ.get("DATABASE_URL")
    if external_url:
        return external_url

    # Otherwise check if we have a container
    if postgres_container and "postgres" in _container_cache:
        container = _container_cache["postgres"]
        # testcontainers returns postgresql+psycopg:// but psycopg3 expects postgresql://
        url = container.get_connection_url()
        url = url.replace("postgresql+psycopg://", "postgresql://")
        return url

    pytest.skip("No database available")


@pytest_asyncio.fixture(scope="session")
async def db_pool(postgres_url) -> AsyncGenerator[psycopg_pool.AsyncConnectionPool, None]:
    """🔄 SHARED CONNECTION POOL: Efficient connection reuse across tests.

    Part of the unified container approach:
    - Session-scoped pool (2-10 connections)
    - Shared by ALL tests for efficiency
    - No connection creation overhead per test
    - Use `db_connection` fixture for test isolation
    """
    # Create connection pool without opening it in constructor to avoid deprecation warning
    pool = psycopg_pool.AsyncConnectionPool(
        postgres_url,
        min_size=2,
        max_size=10,
        timeout=30,
        open=False,  # Don't open in constructor
    )

    # Open the pool explicitly
    await pool.open()

    # Wait for pool to be ready
    await pool.wait()

    # Create base schema if needed
    async with pool.connection() as conn:
        await conn.execute(
            """
            -- Enable required extensions
            CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
            CREATE EXTENSION IF NOT EXISTS "pgcrypto";
            CREATE EXTENSION IF NOT EXISTS "ltree";
        """
        )
        # Try to create pg_fraiseql_cache extension (optional)
        try:
            await conn.execute('CREATE EXTENSION IF NOT EXISTS "pg_fraiseql_cache";')
        except Exception:
            # Extension not installed, skip silently
            pass
        await conn.commit()

    yield pool

    # Cleanup
    await pool.close()


@pytest_asyncio.fixture
async def db_connection(db_pool) -> AsyncGenerator[psycopg.AsyncConnection, None]:
    """Provide an isolated database connection for each test.

    This fixture provides a connection with automatic transaction rollback
    to ensure test isolation. Each test runs in its own transaction that
    is rolled back at the end, leaving the database unchanged.
    """
    async with db_pool.connection() as conn:
        # Start a transaction
        await conn.execute("BEGIN")

        # Set up test-specific configuration
        await conn.execute("SET search_path TO public")

        yield conn

        # Rollback to ensure isolation
        await conn.execute("ROLLBACK")


@pytest_asyncio.fixture
async def db_cursor(db_connection):
    """Provide a cursor for simple database operations."""
    async with db_connection.cursor() as cur:
        yield cur


@pytest.fixture
def create_test_table():
    """Factory fixture to create test tables."""
    created_tables = []

    async def _create_table(conn: psycopg.AsyncConnection, table_name: str, schema: str):
        """Create a test table with the given schema."""
        await conn.execute(f"DROP TABLE IF EXISTS {table_name} CASCADE")
        await conn.execute(schema)
        created_tables.append(table_name)
        return table_name

    return _create_table

    # Cleanup is handled by transaction rollback


@pytest.fixture
def create_test_view():
    """Factory fixture to create test views."""
    created_views = []

    async def _create_view(conn: psycopg.AsyncConnection, view_name: str, query: str):
        """Create a test view with the given query."""
        await conn.execute(f"DROP VIEW IF EXISTS {view_name} CASCADE")
        await conn.execute(f"CREATE VIEW {view_name} AS {query}")
        created_views.append(view_name)
        return view_name

    return _create_view

    # Cleanup is handled by transaction rollback


@pytest.fixture
def create_fraiseql_app_with_db(postgres_url, clear_registry, db_pool):
    """Factory fixture to create FraiseQL apps with real database connection.

    This fixture provides a factory function that creates properly configured
    FraiseQL apps using the real PostgreSQL container and pre-initialized pool.

    Usage:
        def test_something(create_fraiseql_app_with_db):
            app = create_fraiseql_app_with_db(
                types=[MyType],
                queries=[my_query],
                production=False
            )
            client = TestClient(app)
            # Use the app...
    """
    from fraiseql.fastapi.app import create_fraiseql_app
    from fraiseql.fastapi.dependencies import set_db_pool

    def _create_app(**kwargs):
        """Create a FraiseQL app with proper database URL and pool."""
        # Use the real database URL from the container
        kwargs.setdefault("database_url", postgres_url)

        # Create the app
        app = create_fraiseql_app(**kwargs)

        # Manually set the database pool to bypass lifespan issues in tests
        set_db_pool(db_pool)

        return app

    return _create_app


# Alternative fixtures for tests that need committed data
@pytest_asyncio.fixture
async def db_connection_committed(db_pool) -> AsyncGenerator[psycopg.AsyncConnection, None]:
    """Provide a database connection with committed changes.

    Use this fixture when you need changes to persist across queries
    within the same test. The database is still cleaned up after the test.
    """
    async with db_pool.connection() as conn:
        # Generate unique schema for this test
        import uuid

        test_schema = f"test_{uuid.uuid4().hex[:8]}"

        # Create and use test schema
        await conn.execute(f"CREATE SCHEMA {test_schema}")
        await conn.execute(f"SET search_path TO {test_schema}, public")

        yield conn

        # Cleanup schema
        await conn.execute(f"DROP SCHEMA {test_schema} CASCADE")
        await conn.commit()


# Marker for database tests
def pytest_configure(config) -> None:
    """Register custom markers."""
    config.addinivalue_line("markers", "database: mark test as requiring database access")


# Skip database tests if --no-db flag is provided
def pytest_addoption(parser) -> None:
    """Add custom command line options."""
    # Check if option already exists to avoid duplicate registration
    if not any(opt.dest == "no_db" for opt in parser._anonymous.options if hasattr(opt, "dest")):
        parser.addoption(
            "--no-db", action="store_true", default=False, help="Skip database integration tests"
        )


def pytest_collection_modifyitems(config, items) -> None:
    """Modify test collection based on markers."""
    if config.getoption("--no-db"):
        skip_db = pytest.mark.skip(reason="Skipping database tests (--no-db flag)")
        for item in items:
            if "database" in item.keywords:
                item.add_marker(skip_db)
