"""Test configuration and fixtures for blog_simple example.

This file can either run standalone (when running tests directly in blog_simple/)
or integrate with the main test suite (when running from the FraiseQL root).
"""

import sys
from pathlib import Path

# Try to import shared fixtures from main test suite
try:
    # When running from main test suite
    from tests.fixtures.examples.conftest_examples import *  # noqa: F403, F401
    USING_MAIN_FIXTURES = True
except ImportError:
    # When running standalone in blog_simple directory
    USING_MAIN_FIXTURES = False

    # Standalone fixtures for blog_simple
    import asyncio
    import os
    import uuid
    from typing import AsyncGenerator

    import pytest
    import pytest_asyncio
    import psycopg
    from httpx import AsyncClient

    from fraiseql.cqrs import CQRSRepository

    # Test database configuration
    DB_NAME = os.getenv("DB_NAME", "fraiseql_blog_simple_test")
    DB_USER = os.getenv("DB_USER", "fraiseql")
    DB_PASSWORD = os.getenv("DB_PASSWORD", "fraiseql")
    DB_HOST = os.getenv("DB_HOST", "localhost")
    DB_PORT = int(os.getenv("DB_PORT", "5432"))

    TEST_DATABASE_URL = f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{DB_NAME}"

    @pytest.fixture(scope="session")
    def event_loop():
        """Create an instance of the default event loop for the test session."""
        loop = asyncio.get_event_loop_policy().new_event_loop()
        yield loop
        loop.close()

    @pytest_asyncio.fixture(scope="session")
    async def setup_test_db():
        """Setup test database with schema and seed data."""
        admin_url = f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/postgres"

        try:
            admin_conn = await psycopg.AsyncConnection.connect(admin_url)
            await admin_conn.set_autocommit(True)

            # Drop and create test database
            await admin_conn.execute(f"DROP DATABASE IF EXISTS {DB_NAME}")
            await admin_conn.execute(f"CREATE DATABASE {DB_NAME}")
            await admin_conn.close()

            # Connect to test database and setup schema
            test_conn = await psycopg.AsyncConnection.connect(TEST_DATABASE_URL)

            # Read and execute schema
            schema_path = os.path.join(os.path.dirname(__file__), "..", "db", "setup.sql")
            with open(schema_path, "r") as f:
                schema_sql = f.read()
            await test_conn.execute(schema_sql)

            # Read and execute seed data
            seed_path = os.path.join(os.path.dirname(__file__), "..", "db", "seed_data.sql")
            with open(seed_path, "r") as f:
                seed_sql = f.read()
            await test_conn.execute(seed_sql)

            await test_conn.close()

            yield

            # Cleanup - drop test database
            admin_conn = await psycopg.AsyncConnection.connect(admin_url)
            await admin_conn.set_autocommit(True)
            await admin_conn.execute(f"DROP DATABASE IF EXISTS {DB_NAME}")
            await admin_conn.close()

        except Exception as e:
            print(f"Database setup failed: {e}")
            yield

    @pytest_asyncio.fixture
    async def db_connection(setup_test_db) -> AsyncGenerator[psycopg.AsyncConnection, None]:
        """Provide a database connection for testing."""
        conn = await psycopg.AsyncConnection.connect(TEST_DATABASE_URL)
        yield conn
        await conn.close()

    @pytest_asyncio.fixture
    async def db_repo(db_connection) -> AsyncGenerator[CQRSRepository, None]:
        """Provide a CQRS repository for testing."""
        repo = CQRSRepository(db_connection)
        yield repo

    @pytest_asyncio.fixture
    async def test_context(db_repo) -> dict:
        """Provide test context with database and user info."""
        return {
            "db": db_repo,
            "user_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),  # johndoe
            "tenant_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),  # test tenant
        }

    @pytest_asyncio.fixture
    async def app_client(setup_test_db) -> AsyncGenerator[AsyncClient, None]:
        """Provide HTTP client for testing the FastAPI application."""
        try:
            # Try local import first (when running from blog_simple directory)
            from app import app
        except ImportError:
            # Fallback for when running from repository root (CI environment)
            from examples.blog_simple.app import app

        # Override database URL for testing
        os.environ["DB_NAME"] = DB_NAME

        async with AsyncClient(app=app, base_url="http://test") as client:
            yield client

    @pytest_asyncio.fixture
    async def graphql_client(app_client):
        """Provide GraphQL client for testing GraphQL operations."""

        class GraphQLClient:
            def __init__(self, http_client: AsyncClient):
                self.client = http_client

            async def execute(self, query: str, variables: dict = None) -> dict:
                """Execute GraphQL query/mutation."""
                response = await self.client.post(
                    "/graphql",
                    json={
                        "query": query,
                        "variables": variables or {}
                    }
                )
                return response.json()

        yield GraphQLClient(app_client)


# Sample data fixtures
@pytest.fixture
def sample_user_data() -> dict:
    """Sample user data for testing."""
    return {
        "username": "testuser",
        "email": "test@example.com",
        "password": "testpassword123",
        "role": "user",
        "profile_data": {
            "first_name": "Test",
            "last_name": "User",
            "bio": "Test user for automated testing"
        }
    }


@pytest.fixture
def sample_post_data() -> dict:
    """Sample post data for testing."""
    return {
        "title": "Test Blog Post",
        "content": "This is a test blog post with some content for testing purposes.",
        "excerpt": "This is a test excerpt.",
        "status": "draft"
    }


@pytest.fixture
def sample_tag_data() -> dict:
    """Sample tag data for testing."""
    return {
        "name": "Test Tag",
        "color": "#ff0000",
        "description": "A tag for testing purposes"
    }


@pytest.fixture
def sample_comment_data() -> dict:
    """Sample comment data for testing."""
    return {
        "content": "This is a test comment with valuable insights."
    }
