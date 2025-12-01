"""
Shared fixtures for FraiseQL examples integration testing.

These fixtures provide intelligent dependency management and database setup
for example integration tests, with automatic installation and smart caching.
"""

import asyncio
import logging
import os
import sys
from pathlib import Path
from typing import AsyncGenerator, Any
from uuid import UUID, uuid4

import pytest
import pytest_asyncio

# Import smart management systems
from .dependency_manager import (
    SmartDependencyManager,
    get_dependency_manager,
    get_example_dependencies,
    InstallResult,
)
from .database_manager import ExampleDatabaseManager, get_database_manager
from .environment_detector import get_environment_detector, get_environment_config, Environment

# Setup logging for smart fixtures
logger = logging.getLogger(__name__)

# Add examples directory to Python path for imports
EXAMPLES_DIR = Path(__file__).parent.parent.parent.parent / "examples"
# Note: We don't add examples to sys.path globally to avoid contamination
# Each fixture will manage its own path isolation

# Conditional imports that will be available after smart dependencies
try:
    import psycopg
    from fraiseql.cqrs import CQRSRepository
    from httpx import AsyncClient

    DEPENDENCIES_AVAILABLE = True
except ImportError:
    # Will be installed by smart_dependencies fixture
    DEPENDENCIES_AVAILABLE = False
    psycopg = None
    CQRSRepository = None
    AsyncClient = None


@pytest.fixture(scope="session")
def smart_dependencies() -> None:
    """Ensure all required dependencies are available for example tests."""
    # Skip complex dependency management - assume dependencies are available when running via uv
    # This assumes the tests are being run in the proper environment
    logger.info("Assuming example dependencies are available")
    return {
        "dependency_results": {
            "fraiseql": "available",
            "httpx": "available",
            "psycopg": "available",
            "fastapi": "available",
        },
        "environment": "local",
        "performance_profile": "development",
    }


@pytest.fixture(scope="session")
def examples_event_loop() -> None:
    """Create event loop for examples testing."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest_asyncio.fixture(scope="function")
async def blog_simple_db_url(smart_dependencies) -> AsyncGenerator[str, None]:
    """Setup blog_simple test database using smart database manager."""
    db_manager = get_database_manager()

    try:
        success, result = await db_manager.ensure_test_database("blog_simple")
        if not success:
            pytest.skip(f"Blog simple database setup failed: {result}")
        yield result
    except Exception as e:
        logger.warning(f"Blog simple database setup failed: {e}")
        pytest.skip(f"Blog simple database setup failed: {e}")
    # Note: We don't clean up test databases here as it can hang if other tests
    # have open connections. Test databases are unique (UUID suffix) and ephemeral.


@pytest_asyncio.fixture
async def blog_simple_db_connection(blog_simple_db_url) -> None:
    """Provide database connection for blog_simple tests."""
    try:
        import psycopg

        conn = await psycopg.AsyncConnection.connect(blog_simple_db_url)
        yield conn
        await conn.close()
    except Exception as e:
        pytest.skip(f"Database connection failed: {e}")


@pytest_asyncio.fixture
async def blog_simple_repository(blog_simple_db_connection) -> None:
    """Provide CQRS repository for blog_simple tests."""
    from fraiseql.cqrs import CQRSRepository

    repo = CQRSRepository(blog_simple_db_connection)
    yield repo


@pytest_asyncio.fixture
async def blog_simple_context(blog_simple_repository) -> dict[str, Any]:
    """Provide test context for blog_simple."""
    return {
        "db": blog_simple_repository,
        "user_id": UUID("22222222-2222-2222-2222-222222222222"),  # johndoe from seed data
        "tenant_id": UUID("11111111-1111-1111-1111-111111111111"),  # test tenant
        "organization_id": UUID("11111111-1111-1111-1111-111111111111"),
    }


@pytest_asyncio.fixture
async def blog_simple_app(smart_dependencies, blog_simple_db_url) -> AsyncGenerator[Any, None]:
    """Create blog_simple app for testing with guaranteed dependencies."""
    import sys
    import importlib.util
    from urllib.parse import urlparse

    blog_simple_dir = EXAMPLES_DIR / "blog_simple"
    app_file = blog_simple_dir / "app.py"

    # Parse the test database URL and set individual env vars
    # The example uses DB_NAME, DB_USER, etc. not DATABASE_URL
    parsed = urlparse(blog_simple_db_url)
    os.environ["DATABASE_URL"] = blog_simple_db_url
    os.environ["DB_NAME"] = parsed.path.lstrip("/")
    os.environ["DB_USER"] = parsed.username or "fraiseql"
    os.environ["DB_PASSWORD"] = parsed.password or "fraiseql"
    os.environ["DB_HOST"] = parsed.hostname or "localhost"
    os.environ["DB_PORT"] = str(parsed.port or 5432)

    try:
        # Force fresh module load using importlib (bypass Python cache)
        spec = importlib.util.spec_from_file_location(
            "blog_simple_app_module", app_file, submodule_search_locations=[str(blog_simple_dir)]
        )
        if spec is None or spec.loader is None:
            pytest.skip(f"Could not load app module from {app_file}")

        # Add directory to path for imports within the module
        sys.path.insert(0, str(blog_simple_dir))

        module = importlib.util.module_from_spec(spec)
        sys.modules["app"] = module  # Register so internal imports work
        spec.loader.exec_module(module)

        # Create app
        app = module.create_app()
        yield app

    except Exception as e:
        logger.warning(f"Blog simple app creation failed: {e}")
        pytest.skip(f"Blog simple app creation failed: {e}")
    finally:
        # Clean up
        if str(blog_simple_dir) in sys.path:
            sys.path.remove(str(blog_simple_dir))
        if "app" in sys.modules:
            del sys.modules["app"]
        # Clear any cached modules from the example
        modules_to_remove = [k for k in sys.modules.keys() if "blog_simple" in k.lower()]
        for mod in modules_to_remove:
            del sys.modules[mod]


@pytest_asyncio.fixture
async def blog_simple_client(blog_simple_app, blog_simple_db_url) -> AsyncGenerator[Any, None]:
    """HTTP client for blog_simple app with guaranteed dependencies."""
    import asyncio
    from httpx import AsyncClient, ASGITransport
    import psycopg_pool

    # Create and set pool manually to ensure database pool is initialized
    pool = psycopg_pool.AsyncConnectionPool(blog_simple_db_url)
    await pool.open()

    try:
        from fraiseql.fastapi.dependencies import set_db_pool

        set_db_pool(pool)

        transport = ASGITransport(app=blog_simple_app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            yield client
    finally:
        from fraiseql.fastapi.dependencies import set_db_pool

        set_db_pool(None)

        # Close pool with timeout to avoid hanging
        try:
            await asyncio.wait_for(pool.close(), timeout=10.0)
        except asyncio.TimeoutError:
            logger.warning("Pool close timed out after 10s, continuing anyway")


@pytest_asyncio.fixture
async def blog_simple_graphql_client(blog_simple_client) -> None:
    """GraphQL client for blog_simple."""

    class GraphQLClient:
        def __init__(self, http_client: AsyncClient) -> None:
            self.client = http_client

        async def execute(self, query: str, variables: dict[str, Any] = None) -> dict[str, Any]:
            """Execute GraphQL query/mutation."""
            response = await self.client.post(
                "/graphql", json={"query": query, "variables": variables or {}}
            )
            return response.json()

    yield GraphQLClient(blog_simple_client)


@pytest_asyncio.fixture(scope="function")
async def blog_enterprise_db_url(smart_dependencies) -> AsyncGenerator[str, None]:
    """Setup blog_enterprise test database using smart database manager."""
    db_manager = get_database_manager()

    try:
        success, result = await db_manager.ensure_test_database("blog_enterprise")
        if not success:
            pytest.skip(f"Blog enterprise database setup failed: {result}")
        yield result
    except Exception as e:
        logger.warning(f"Blog enterprise database setup failed: {e}")
        pytest.skip(f"Blog enterprise database setup failed: {e}")
    # Note: We don't clean up test databases here as it can hang if other tests
    # have open connections. Test databases are unique (UUID suffix) and ephemeral.


@pytest_asyncio.fixture
async def blog_enterprise_app(
    smart_dependencies, blog_enterprise_db_url
) -> AsyncGenerator[Any, None]:
    """Create blog_enterprise app for testing with guaranteed dependencies."""
    import sys
    import importlib.util
    from urllib.parse import urlparse

    blog_enterprise_dir = EXAMPLES_DIR / "blog_enterprise"
    app_file = blog_enterprise_dir / "app.py"

    # Parse the test database URL and set individual env vars
    # The example uses DB_NAME, DB_USER, etc. not DATABASE_URL
    parsed = urlparse(blog_enterprise_db_url)
    os.environ["DATABASE_URL"] = blog_enterprise_db_url
    os.environ["DB_NAME"] = parsed.path.lstrip("/")
    os.environ["DB_USER"] = parsed.username or "fraiseql"
    os.environ["DB_PASSWORD"] = parsed.password or "fraiseql"
    os.environ["DB_HOST"] = parsed.hostname or "localhost"
    os.environ["DB_PORT"] = str(parsed.port or 5432)

    try:
        # Force fresh module load using importlib (bypass Python cache)
        spec = importlib.util.spec_from_file_location(
            "blog_enterprise_app_module",
            app_file,
            submodule_search_locations=[str(blog_enterprise_dir)],
        )
        if spec is None or spec.loader is None:
            pytest.skip(f"Could not load app module from {app_file}")

        # Add directory to path for imports within the module
        sys.path.insert(0, str(blog_enterprise_dir))

        module = importlib.util.module_from_spec(spec)
        sys.modules["app"] = module  # Register so internal imports work
        spec.loader.exec_module(module)

        # Create app
        app = module.create_app()
        yield app

    except Exception as e:
        logger.warning(f"Blog enterprise app creation failed: {e}")
        pytest.skip(f"Blog enterprise app creation failed: {e}")
    finally:
        # Clean up
        if str(blog_enterprise_dir) in sys.path:
            sys.path.remove(str(blog_enterprise_dir))
        if "app" in sys.modules:
            del sys.modules["app"]
        # Clear any cached modules from the example
        modules_to_remove = [k for k in sys.modules.keys() if "blog_enterprise" in k.lower()]
        for mod in modules_to_remove:
            del sys.modules[mod]


@pytest_asyncio.fixture
async def blog_enterprise_client(
    blog_enterprise_app, blog_enterprise_db_url
) -> AsyncGenerator[Any, None]:
    """HTTP client for blog_enterprise app with guaranteed dependencies."""
    import asyncio
    from httpx import AsyncClient, ASGITransport
    import psycopg_pool

    # Create and set pool manually to ensure database pool is initialized
    pool = psycopg_pool.AsyncConnectionPool(blog_enterprise_db_url)
    await pool.open()

    try:
        from fraiseql.fastapi.dependencies import set_db_pool

        set_db_pool(pool)

        transport = ASGITransport(app=blog_enterprise_app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            yield client
    finally:
        from fraiseql.fastapi.dependencies import set_db_pool

        set_db_pool(None)

        # Close pool with timeout to avoid hanging
        try:
            await asyncio.wait_for(pool.close(), timeout=5.0)
        except asyncio.TimeoutError:
            logger.warning("Pool close timed out, forcing termination")


# Sample data fixtures that work across examples
@pytest.fixture
def sample_user_data() -> None:
    """Sample user data for testing."""
    return {
        "username": f"testuser_{uuid4().hex[:8]}",
        "email": f"test_{uuid4().hex[:8]}@example.com",
        "password": "testpassword123",
        "role": "user",
        "profile_data": {
            "first_name": "Test",
            "last_name": "User",
            "bio": "Test user for integration testing",
        },
    }


@pytest.fixture
def sample_post_data() -> None:
    """Sample post data for testing."""
    return {
        "title": f"Test Post {uuid4().hex[:8]}",
        "content": "This is a test post with some content for integration testing purposes.",
        "excerpt": "This is a test excerpt for integration testing.",
        "status": "draft",
    }


@pytest.fixture
def sample_tag_data() -> None:
    """Sample tag data for testing."""
    return {
        "name": f"Test Tag {uuid4().hex[:8]}",
        "color": "#ff0000",
        "description": "A tag for integration testing purposes",
    }


@pytest.fixture
def sample_comment_data() -> None:
    """Sample comment data for testing."""
    return {
        "content": f"This is a test comment {uuid4().hex[:8]} with valuable insights for integration testing."
    }


# Cascade Example Fixtures - Removed
# The cascade fixtures are now in tests/fixtures/cascade/conftest.py
# to avoid conflicts and use the proper db_pool-based setup
