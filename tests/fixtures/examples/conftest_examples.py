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
from typing import AsyncGenerator, Dict, Any
from uuid import UUID, uuid4

import pytest

# Import smart management systems
from .dependency_manager import (
    SmartDependencyManager, 
    get_dependency_manager,
    get_example_dependencies,
    InstallResult
)
from .database_manager import (
    ExampleDatabaseManager,
    get_database_manager
)
from .environment_detector import (
    get_environment_detector,
    get_environment_config,
    Environment
)

# Setup logging for smart fixtures
logger = logging.getLogger(__name__)

# Add examples directory to Python path for imports
EXAMPLES_DIR = Path(__file__).parent.parent.parent.parent / "examples"
sys.path.insert(0, str(EXAMPLES_DIR))

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
    """Ensure all required dependencies are installed for example tests."""
    env_detector = get_environment_detector()
    env_config = env_detector.get_environment_config()
    dependency_manager = get_dependency_manager()
    
    logger.info(f"Smart dependencies running in {env_config.environment.value} environment")
    
    # Get required dependencies for examples
    required_deps = [dep.name for dep in get_example_dependencies()]
    
    # Try to ensure dependencies are available
    success, results = dependency_manager.ensure_dependencies(
        required_deps,
        context=None  # Will use detected environment config
    )
    
    if not success:
        # Log detailed information about failed dependencies
        failed_deps = [name for name, result in results.items() 
                      if result == InstallResult.FAILED]
        skipped_deps = [name for name, result in results.items() 
                       if result == InstallResult.SKIPPED]
        
        error_msg = f"Smart dependency management failed"
        if failed_deps:
            error_msg += f"\nFailed to install: {', '.join(failed_deps)}"
        if skipped_deps:
            error_msg += f"\nSkipped (auto-install disabled): {', '.join(skipped_deps)}"
        
        # Include environment context for debugging
        debug_info = env_detector.get_debug_info()
        error_msg += f"\nEnvironment: {debug_info['detected_environment']}"
        error_msg += f"\nAuto-install enabled: {debug_info['config']['auto_install_dependencies']}"
        
        pytest.skip(error_msg)
    
    # Post-install validation
    try:
        import fraiseql, httpx, psycopg, fastapi
        logger.info("All example dependencies validated successfully")
        return {
            'dependency_results': results,
            'environment': env_config.environment,
            'performance_profile': env_config.performance_profile
        }
    except ImportError as e:
        pytest.skip(f"Dependency validation failed after installation: {e}")


@pytest.fixture(scope="session")
def examples_event_loop():
    """Create event loop for examples testing."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest.fixture(scope="session")
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


@pytest.fixture
async def blog_simple_db_connection(blog_simple_db_url):
    """Provide database connection for blog_simple tests."""
    try:
        import psycopg
        conn = await psycopg.AsyncConnection.connect(blog_simple_db_url)
        yield conn
        await conn.close()
    except Exception as e:
        pytest.skip(f"Database connection failed: {e}")


@pytest.fixture
async def blog_simple_repository(blog_simple_db_connection):
    """Provide CQRS repository for blog_simple tests."""
    from fraiseql.cqrs import CQRSRepository
    repo = CQRSRepository(blog_simple_db_connection)
    yield repo


@pytest.fixture
async def blog_simple_context(blog_simple_repository) -> Dict[str, Any]:
    """Provide test context for blog_simple."""
    return {
        "db": blog_simple_repository,
        "user_id": UUID("22222222-2222-2222-2222-222222222222"),  # johndoe from seed data
        "tenant_id": UUID("11111111-1111-1111-1111-111111111111"),  # test tenant
        "organization_id": UUID("11111111-1111-1111-1111-111111111111"),
    }


@pytest.fixture
async def blog_simple_app(smart_dependencies, blog_simple_db_url):
    """Create blog_simple app for testing with guaranteed dependencies."""
    blog_simple_path = None
    try:
        # Import blog_simple app - dependencies guaranteed by smart_dependencies fixture
        blog_simple_path = EXAMPLES_DIR / "blog_simple"
        sys.path.insert(0, str(blog_simple_path))

        from app import create_app

        # Override database settings for testing 
        db_name = blog_simple_db_url.split("/")[-1]
        os.environ["DB_NAME"] = db_name
        os.environ["DATABASE_URL"] = blog_simple_db_url
        os.environ["ENV"] = "test"

        app = create_app()
        logger.info(f"Successfully created blog_simple app with database: {db_name}")
        yield app

    except Exception as e:
        logger.error(f"Failed to create blog_simple app: {e}")
        pytest.skip(f"Failed to create blog_simple app: {e}")
    finally:
        # Clean up sys.path
        if blog_simple_path and str(blog_simple_path) in sys.path:
            sys.path.remove(str(blog_simple_path))


@pytest.fixture
async def blog_simple_client(blog_simple_app):
    """HTTP client for blog_simple app with guaranteed dependencies."""
    # Dependencies guaranteed by smart_dependencies fixture
    from httpx import AsyncClient
    
    async with AsyncClient(app=blog_simple_app, base_url="http://test") as client:
        yield client


@pytest.fixture
async def blog_simple_graphql_client(blog_simple_client):
    """GraphQL client for blog_simple."""

    class GraphQLClient:
        def __init__(self, http_client: AsyncClient):
            self.client = http_client

        async def execute(self, query: str, variables: Dict[str, Any] = None) -> Dict[str, Any]:
            """Execute GraphQL query/mutation."""
            response = await self.client.post(
                "/graphql",
                json={
                    "query": query,
                    "variables": variables or {}
                }
            )
            return response.json()

    yield GraphQLClient(blog_simple_client)


@pytest.fixture(scope="session")
async def blog_enterprise_db_url():
    """Setup blog_enterprise test database."""
    db_name = f"fraiseql_blog_enterprise_test_{uuid4().hex[:8]}"
    admin_url = "postgresql://fraiseql:fraiseql@localhost:5432/postgres"

    try:
        # Create test database
        admin_conn = await psycopg.AsyncConnection.connect(admin_url)
        await admin_conn.set_autocommit(True)
        await admin_conn.execute(f"DROP DATABASE IF EXISTS {db_name}")
        await admin_conn.execute(f"CREATE DATABASE {db_name}")
        await admin_conn.close()

        test_url = f"postgresql://fraiseql:fraiseql@localhost:5432/{db_name}"

        # For now, just create empty database - enterprise example needs more setup
        # This can be expanded when the enterprise schema is complete

        yield test_url

        # Cleanup
        admin_conn = await psycopg.AsyncConnection.connect(admin_url)
        await admin_conn.set_autocommit(True)
        await admin_conn.execute(f"DROP DATABASE IF EXISTS {db_name}")
        await admin_conn.close()

    except Exception as e:
        print(f"Failed to setup blog_enterprise test database: {e}")
        yield f"postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test_fallback"


@pytest.fixture
async def blog_enterprise_app():
    """Create blog_enterprise app for testing."""
    # Check if FraiseQL is available first
    try:
        import fraiseql
    except ImportError:
        pytest.skip("FraiseQL not installed - skipping blog_enterprise integration tests")

    blog_enterprise_path = None
    try:
        # Import blog_enterprise app
        blog_enterprise_path = EXAMPLES_DIR / "blog_enterprise"
        sys.path.insert(0, str(blog_enterprise_path))

        from app import create_app

        # Override database settings for testing
        os.environ["DB_NAME"] = "fraiseql_blog_enterprise_test"
        os.environ["ENV"] = "test"

        app = create_app()
        yield app

    except (ImportError, ModuleNotFoundError) as e:
        pytest.skip(f"Could not import blog_enterprise app (likely missing dependencies): {e}")
    except Exception as e:
        pytest.skip(f"Failed to create blog_enterprise app: {e}")
    finally:
        # Clean up sys.path
        if blog_enterprise_path and str(blog_enterprise_path) in sys.path:
            sys.path.remove(str(blog_enterprise_path))


@pytest.fixture
async def blog_enterprise_client(blog_enterprise_app):
    """HTTP client for blog_enterprise app with guaranteed dependencies."""
    # Dependencies guaranteed by smart_dependencies fixture
    from httpx import AsyncClient
    
    async with AsyncClient(app=blog_enterprise_app, base_url="http://test") as client:
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
            "bio": "Test user for integration testing"
        }
    }


@pytest.fixture
def sample_post_data():
    """Sample post data for testing."""
    return {
        "title": f"Test Post {uuid4().hex[:8]}",
        "content": "This is a test post with some content for integration testing purposes.",
        "excerpt": "This is a test excerpt for integration testing.",
        "status": "draft"
    }


@pytest.fixture
def sample_tag_data():
    """Sample tag data for testing."""
    return {
        "name": f"Test Tag {uuid4().hex[:8]}",
        "color": "#ff0000",
        "description": "A tag for integration testing purposes"
    }


@pytest.fixture
def sample_comment_data():
    """Sample comment data for testing."""
    return {
        "content": f"This is a test comment {uuid4().hex[:8]} with valuable insights for integration testing."
    }
