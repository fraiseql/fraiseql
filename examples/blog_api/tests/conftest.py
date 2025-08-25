"""Test configuration and fixtures for blog API tests."""

import asyncio
import os

# Add parent directory to path for imports
import sys
from collections.abc import AsyncGenerator
from pathlib import Path
from uuid import uuid4

import psycopg
import pytest
import pytest_asyncio
from fastapi.testclient import TestClient
from httpx import AsyncClient

from fraiseql.auth import UserContext

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

# Import after path is set up
from db import BlogRepository
from models import User

# Test database configuration
TEST_DATABASE_URL = os.getenv("TEST_DATABASE_URL", "postgresql://localhost/blog_test")


# Event loop fixture removed - let pytest-asyncio handle it


@pytest_asyncio.fixture(scope="function")
async def db_connection() -> AsyncGenerator[psycopg.AsyncConnection, None]:
    """Get a database connection for tests."""
    # Use direct connection instead of pool for simplicity in tests
    conn = await psycopg.AsyncConnection.connect(TEST_DATABASE_URL)
    try:
        yield conn
    finally:
        await conn.close()


@pytest_asyncio.fixture(scope="function")
async def clean_db(
    db_connection: psycopg.AsyncConnection,
) -> AsyncGenerator[None, None]:
    """Clean the database before and after each test."""
    # Clean all tables
    await db_connection.execute(
        "TRUNCATE TABLE tb_comments, tb_posts, tb_users CASCADE",
    )
    await db_connection.commit()

    yield

    # Clean again after test
    await db_connection.execute(
        "TRUNCATE TABLE tb_comments, tb_posts, tb_users CASCADE",
    )
    await db_connection.commit()


@pytest_asyncio.fixture
async def blog_repo(db_connection: psycopg.AsyncConnection) -> BlogRepository:
    """Create a BlogRepository instance."""
    return BlogRepository(db_connection)


@pytest_asyncio.fixture
async def test_user(blog_repo: BlogRepository) -> dict:
    """Create a test user."""
    result = await blog_repo.create_user(
        {
            "email": f"test_{uuid4()}@example.com",
            "name": "Test User",
            "bio": "Test bio",
            "password_hash": "hashed_password",
        },
    )

    assert result["success"]
    user_data = await blog_repo.get_user_by_id(result["user_id"])
    return user_data


@pytest_asyncio.fixture
async def admin_user(blog_repo: BlogRepository) -> dict:
    """Create an admin user."""
    result = await blog_repo.create_user(
        {
            "email": f"admin_{uuid4()}@example.com",
            "name": "Admin User",
            "bio": "Admin bio",
            "password_hash": "hashed_password",
        },
    )

    assert result["success"]

    # Update user to have admin role
    await blog_repo.connection.execute(
        "UPDATE tb_users SET roles = ARRAY['user', 'admin'] WHERE id = %s",
        (result["user_id"],),
    )

    user_data = await blog_repo.get_user_by_id(result["user_id"])
    return user_data


@pytest.fixture
def auth_context(test_user: dict) -> UserContext:
    """Create an authenticated user context."""
    return UserContext(
        user_id=str(test_user["id"]),
        email=test_user["email"],
        roles=test_user["roles"],
        permissions=[],
    )


@pytest.fixture
def admin_context(admin_user: dict) -> UserContext:
    """Create an admin user context."""
    return UserContext(
        user_id=str(admin_user["id"]),
        email=admin_user["email"],
        roles=admin_user["roles"],
        permissions=["admin"],
    )


@pytest.fixture
def test_client() -> TestClient:
    """Create a test client for the FastAPI app."""
    # Import here to avoid circular dependencies
    from app import app

    return TestClient(app)


@pytest_asyncio.fixture
async def async_client() -> AsyncGenerator[AsyncClient, None]:
    """Create an async test client for the FastAPI app."""
    # Import here to avoid circular dependencies
    from app import app

    async with AsyncClient(app=app, base_url="http://test") as client:
        yield client


@pytest.fixture
def graphql_headers() -> dict:
    """Common headers for GraphQL requests."""
    return {
        "Content-Type": "application/json",
    }


@pytest.fixture
def auth_headers(test_user: dict) -> dict:
    """Headers with authentication for GraphQL requests."""
    # In a real app, you'd generate a proper JWT token
    # For testing, we'll use a mock token
    return {
        "Content-Type": "application/json",
        "Authorization": f"Bearer test-token-{test_user['id']}",
    }


@pytest_asyncio.fixture
async def create_test_post(blog_repo: BlogRepository, test_user: dict):
    """Factory fixture to create test posts."""
    created_posts = []

    async def _create_post(
        title: str = "Test Post",
        content: str = "Test content",
        is_published: bool = True,
        tags: list[str] | None = None,
    ):
        result = await blog_repo.create_post(
            {
                "author_id": str(test_user["id"]),
                "title": title,
                "content": content,
                "excerpt": f"Excerpt for {title}",
                "tags": tags or ["test"],
                "is_published": is_published,
            },
        )

        assert result["success"]
        post = await blog_repo.get_post_by_id(result["post_id"])
        created_posts.append(post)
        return post

    yield _create_post

    # Cleanup is handled by clean_db fixture


@pytest_asyncio.fixture
async def create_test_comment(blog_repo: BlogRepository, test_user: User):
    """Factory fixture to create test comments."""

    async def _create_comment(
        post_id: str, content: str = "Test comment", parent_id: str | None = None,
    ):
        result = await blog_repo.create_comment(
            {
                "post_id": post_id,
                "author_id": str(test_user.id),
                "content": content,
                "parent_id": parent_id,
            },
        )

        assert result["success"]
        return result["comment_id"]

    return _create_comment
