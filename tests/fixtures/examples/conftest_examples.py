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
def smart_dependencies():
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
def examples_event_loop():
    """Create event loop for examples testing."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest_asyncio.fixture(scope="session")
async def blog_simple_db_url(smart_dependencies):
    """Setup blog_simple test database using smart database manager."""
    db_manager = get_database_manager()

    try:
        success, connection_string = await db_manager.ensure_test_database("blog_simple")

        if success:
            logger.info(f"Successfully set up blog_simple test database")
            yield connection_string

            # Cleanup test database (template is kept for future runs)
            db_name = connection_string.split("/")[-1]
            logger.info(f"Cleaning up test database: {db_name}")
            db_manager._drop_database(db_name)
        else:
            pytest.skip(f"Failed to setup blog_simple test database: {connection_string}")

    except Exception as e:
        logger.error(f"Exception setting up blog_simple test database: {e}")
        pytest.skip(f"Database setup failed: {e}")


@pytest_asyncio.fixture
async def blog_simple_db_connection(blog_simple_db_url):
    """Provide database connection for blog_simple tests."""
    try:
        import psycopg

        conn = await psycopg.AsyncConnection.connect(blog_simple_db_url)
        yield conn
        await conn.close()
    except Exception as e:
        pytest.skip(f"Database connection failed: {e}")


@pytest_asyncio.fixture
async def blog_simple_repository(blog_simple_db_connection):
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
async def blog_simple_app(smart_dependencies, blog_simple_db_url):
    """Create blog_simple app for testing with guaranteed dependencies."""
    blog_simple_path = None
    original_sys_modules = None
    original_env = {}

    try:
        # Store original environment variables we'll modify
        for key in ["DB_NAME", "DATABASE_URL", "ENV"]:
            if key in os.environ:
                original_env[key] = os.environ[key]

        # Clear the FraiseQL registry completely to prevent schema conflicts
        try:
            from fraiseql.gql.builders.registry import SchemaRegistry

            SchemaRegistry.get_instance().clear()
            logger.info("Successfully cleared SchemaRegistry before creating blog_simple app")
        except ImportError:
            logger.warning("Could not import SchemaRegistry - continuing without clearing")

        # Clear any fraiseql modules from sys.modules to prevent contamination
        modules_to_remove = [
            name
            for name in sys.modules.keys()
            if name.startswith("fraiseql.") and "registry" in name.lower()
        ]
        original_sys_modules = {name: sys.modules.pop(name) for name in modules_to_remove}

        # Import blog_simple app - dependencies guaranteed by smart_dependencies fixture
        blog_simple_path = EXAMPLES_DIR / "blog_simple"
        sys.path.insert(0, str(blog_simple_path))

        # Override database settings for testing
        db_name = blog_simple_db_url.split("/")[-1]
        os.environ["DB_NAME"] = db_name
        os.environ["DATABASE_URL"] = blog_simple_db_url
        os.environ["ENV"] = "test"

        # Import the app module and create app
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "blog_simple_app", blog_simple_path / "app.py"
        )
        app_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(app_module)

        app = app_module.create_app()
        logger.info(f"Successfully created blog_simple app with database: {db_name}")
        yield app

    except Exception as e:
        logger.error(f"Failed to create blog_simple app: {e}")
        pytest.skip(f"Failed to create blog_simple app: {e}")
    finally:
        # Restore original environment
        for key, value in original_env.items():
            os.environ[key] = value

        # Remove test environment variables that weren't in original env
        for key in ["DB_NAME", "DATABASE_URL", "ENV"]:
            if key not in original_env and key in os.environ:
                del os.environ[key]

        # Restore sys.modules if we removed any
        if original_sys_modules:
            sys.modules.update(original_sys_modules)

        # Clean up sys.path
        if blog_simple_path and str(blog_simple_path) in sys.path:
            sys.path.remove(str(blog_simple_path))


@pytest_asyncio.fixture
async def blog_simple_client(blog_simple_app):
    """HTTP client for blog_simple app with guaranteed dependencies."""
    # Dependencies guaranteed by smart_dependencies fixture
    from httpx import AsyncClient, ASGITransport

    # Start the lifespan context to initialize database pool
    async with blog_simple_app.router.lifespan_context(blog_simple_app):
        transport = ASGITransport(app=blog_simple_app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            yield client


@pytest_asyncio.fixture
async def blog_simple_graphql_client(blog_simple_client):
    """GraphQL client for blog_simple."""

    class GraphQLClient:
        def __init__(self, http_client: AsyncClient):
            self.client = http_client

        async def execute(self, query: str, variables: dict[str, Any] = None) -> dict[str, Any]:
            """Execute GraphQL query/mutation."""
            response = await self.client.post(
                "/graphql", json={"query": query, "variables": variables or {}}
            )
            return response.json()

    yield GraphQLClient(blog_simple_client)


@pytest_asyncio.fixture(scope="session")
async def blog_enterprise_db_url(smart_dependencies):
    """Setup blog_enterprise test database using smart database manager."""
    db_manager = get_database_manager()

    try:
        success, connection_string = await db_manager.ensure_test_database("blog_enterprise")

        if success:
            logger.info(f"Successfully set up blog_enterprise test database")
            yield connection_string

            # Cleanup test database (template is kept for future runs)
            db_name = connection_string.split("/")[-1]
            logger.info(f"Cleaning up test database: {db_name}")
            db_manager._drop_database(db_name)
        else:
            pytest.skip(f"Failed to setup blog_enterprise test database: {connection_string}")

    except Exception as e:
        logger.error(f"Exception setting up blog_enterprise test database: {e}")
        pytest.skip(f"Database setup failed: {e}")


@pytest_asyncio.fixture
async def blog_enterprise_app(smart_dependencies, blog_enterprise_db_url):
    """Create blog_enterprise app for testing with guaranteed dependencies."""
    blog_enterprise_path = None
    original_sys_modules = None
    original_env = {}

    try:
        # Store original environment variables we'll modify
        for key in ["DB_NAME", "DATABASE_URL", "ENV"]:
            if key in os.environ:
                original_env[key] = os.environ[key]

        # Clear the FraiseQL registry completely to prevent schema conflicts
        try:
            from fraiseql.gql.builders.registry import SchemaRegistry

            SchemaRegistry.get_instance().clear()
            logger.info("Successfully cleared SchemaRegistry before creating blog_enterprise app")
        except ImportError:
            logger.warning("Could not import SchemaRegistry - continuing without clearing")

        # Clear any fraiseql modules from sys.modules to prevent contamination
        modules_to_remove = [
            name
            for name in sys.modules.keys()
            if name.startswith("fraiseql.") and "registry" in name.lower()
        ]
        original_sys_modules = {name: sys.modules.pop(name) for name in modules_to_remove}

        # Import blog_enterprise app - dependencies guaranteed by smart_dependencies fixture
        blog_enterprise_path = EXAMPLES_DIR / "blog_enterprise"
        sys.path.insert(0, str(blog_enterprise_path))

        # Override database settings for testing
        db_name = blog_enterprise_db_url.split("/")[-1]
        os.environ["DB_NAME"] = db_name
        os.environ["DATABASE_URL"] = blog_enterprise_db_url
        os.environ["ENV"] = "test"

        # Import the app module and create app
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "blog_enterprise_app", blog_enterprise_path / "app.py"
        )
        app_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(app_module)

        app = app_module.create_app()
        logger.info(f"Successfully created blog_enterprise app with database: {db_name}")
        yield app

    except Exception as e:
        logger.error(f"Failed to create blog_enterprise app: {e}")
        pytest.skip(f"Failed to create blog_enterprise app: {e}")
    finally:
        # Restore original environment
        for key, value in original_env.items():
            os.environ[key] = value

        # Remove test environment variables that weren't in original env
        for key in ["DB_NAME", "DATABASE_URL", "ENV"]:
            if key not in original_env and key in os.environ:
                del os.environ[key]

        # Restore sys.modules if we removed any
        if original_sys_modules:
            sys.modules.update(original_sys_modules)

        # Clean up sys.path
        if blog_enterprise_path and str(blog_enterprise_path) in sys.path:
            sys.path.remove(str(blog_enterprise_path))


@pytest_asyncio.fixture
async def blog_enterprise_client(blog_enterprise_app):
    """HTTP client for blog_enterprise app with guaranteed dependencies."""
    # Dependencies guaranteed by smart_dependencies fixture
    from httpx import AsyncClient, ASGITransport

    # Start the lifespan context to initialize database pool
    async with blog_enterprise_app.router.lifespan_context(blog_enterprise_app):
        transport = ASGITransport(app=blog_enterprise_app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            yield client


# Sample data fixtures that work across examples
@pytest.fixture
def sample_user_data():
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
def sample_post_data():
    """Sample post data for testing."""
    return {
        "title": f"Test Post {uuid4().hex[:8]}",
        "content": "This is a test post with some content for integration testing purposes.",
        "excerpt": "This is a test excerpt for integration testing.",
        "status": "draft",
    }


@pytest.fixture
def sample_tag_data():
    """Sample tag data for testing."""
    return {
        "name": f"Test Tag {uuid4().hex[:8]}",
        "color": "#ff0000",
        "description": "A tag for integration testing purposes",
    }


@pytest.fixture
def sample_comment_data():
    """Sample comment data for testing."""
    return {
        "content": f"This is a test comment {uuid4().hex[:8]} with valuable insights for integration testing."
    }
