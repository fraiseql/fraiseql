"""
Shared fixtures for FraiseQL examples integration testing.

These fixtures allow examples to be tested as part of the main test suite
while maintaining isolation and proper setup/teardown.
"""

import asyncio
import os
import sys
from pathlib import Path
from typing import AsyncGenerator, Dict, Any
from uuid import UUID, uuid4

import pytest
import psycopg
from httpx import AsyncClient

from fraiseql.cqrs import CQRSRepository


# Add examples directory to Python path for imports
EXAMPLES_DIR = Path(__file__).parent.parent.parent.parent / "examples"
sys.path.insert(0, str(EXAMPLES_DIR))


@pytest.fixture(scope="session")
def examples_event_loop():
    """Create event loop for examples testing."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest.fixture(scope="session")
async def blog_simple_db_url():
    """Setup blog_simple test database."""
    db_name = f"fraiseql_blog_simple_test_{uuid4().hex[:8]}"
    admin_url = "postgresql://fraiseql:fraiseql@localhost:5432/postgres"

    try:
        # Create test database
        admin_conn = await psycopg.AsyncConnection.connect(admin_url)
        await admin_conn.set_autocommit(True)
        await admin_conn.execute(f"DROP DATABASE IF EXISTS {db_name}")
        await admin_conn.execute(f"CREATE DATABASE {db_name}")
        await admin_conn.close()

        test_url = f"postgresql://fraiseql:fraiseql@localhost:5432/{db_name}"

        # Setup schema and seed data
        test_conn = await psycopg.AsyncConnection.connect(test_url)

        # Read schema
        schema_path = EXAMPLES_DIR / "blog_simple" / "db" / "setup.sql"
        if schema_path.exists():
            with open(schema_path, "r") as f:
                await test_conn.execute(f.read())

        # Read seed data
        seed_path = EXAMPLES_DIR / "blog_simple" / "db" / "seed_data.sql"
        if seed_path.exists():
            with open(seed_path, "r") as f:
                await test_conn.execute(f.read())

        await test_conn.close()

        yield test_url

        # Cleanup
        admin_conn = await psycopg.AsyncConnection.connect(admin_url)
        await admin_conn.set_autocommit(True)
        await admin_conn.execute(f"DROP DATABASE IF EXISTS {db_name}")
        await admin_conn.close()

    except Exception as e:
        print(f"Failed to setup blog_simple test database: {e}")
        # Provide a fallback URL for tests that can handle missing DB
        yield f"postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test_fallback"


@pytest.fixture
async def blog_simple_db_connection(blog_simple_db_url) -> AsyncGenerator[psycopg.AsyncConnection, None]:
    """Provide database connection for blog_simple tests."""
    try:
        conn = await psycopg.AsyncConnection.connect(blog_simple_db_url)
        yield conn
        await conn.close()
    except Exception as e:
        pytest.skip(f"Database connection failed: {e}")


@pytest.fixture
async def blog_simple_repository(blog_simple_db_connection) -> AsyncGenerator[CQRSRepository, None]:
    """Provide CQRS repository for blog_simple tests."""
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
async def blog_simple_app():
    """Create blog_simple app for testing."""
    try:
        # Import blog_simple app
        blog_simple_path = EXAMPLES_DIR / "blog_simple"
        sys.path.insert(0, str(blog_simple_path))

        from app import create_app

        # Override database settings for testing
        os.environ["DB_NAME"] = "fraiseql_blog_simple_test"

        app = create_app()
        yield app

    except ImportError as e:
        pytest.skip(f"Could not import blog_simple app: {e}")
    finally:
        # Clean up sys.path
        if str(blog_simple_path) in sys.path:
            sys.path.remove(str(blog_simple_path))


@pytest.fixture
async def blog_simple_client(blog_simple_app) -> AsyncGenerator[AsyncClient, None]:
    """HTTP client for blog_simple app."""
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

    except ImportError as e:
        pytest.skip(f"Could not import blog_enterprise app: {e}")
    finally:
        # Clean up sys.path
        if str(blog_enterprise_path) in sys.path:
            sys.path.remove(str(blog_enterprise_path))


@pytest.fixture
async def blog_enterprise_client(blog_enterprise_app) -> AsyncGenerator[AsyncClient, None]:
    """HTTP client for blog_enterprise app."""
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
