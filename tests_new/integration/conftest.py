"""Integration test configuration and fixtures.

This module provides specialized fixtures and configuration for integration tests
that require real database connections, external services, or complex component
interactions. Integration tests validate that different parts of FraiseQL work
together correctly.
"""

import json
import os
import time
from typing import AsyncGenerator

import pytest
import pytest_asyncio
from psycopg.types.json import Json

# Import parent conftest fixtures
from tests_new.conftest import *


@pytest.fixture(scope="session")
async def db_schema_setup(db_pool):
    """Setup database schema for integration tests."""
    async with db_pool.connection() as conn:
        # Read and execute schema SQL
        schema_path = os.path.join(os.path.dirname(__file__), "../e2e/blog_demo/schema.sql")
        with open(schema_path, 'r') as f:
            schema_sql = f.read()

        # Execute schema creation
        await conn.execute(schema_sql)
        await conn.commit()

    yield

    # Cleanup is handled by container teardown


@pytest.fixture
async def db_with_schema(db_connection, db_schema_setup):
    """Database connection with schema setup."""
    yield db_connection


@pytest.fixture(scope="session")
def integration_config():
    """Configuration specific to integration tests."""
    return {
        "database": {
            "require_real_db": True,
            "use_transactions": True,
            "cleanup_after_tests": True,
        },
        "external_services": {
            "timeout": 30,
            "retry_attempts": 3,
            "mock_unavailable": True,
        },
        "performance": {
            "max_query_time": 1.0,  # More lenient for integration
            "max_connection_time": 5.0,
        }
    }


@pytest_asyncio.fixture
async def integration_app(fraiseql_app_factory, postgres_url):
    """FraiseQL application configured for integration testing."""
    from tests_new.e2e.blog_demo.models import (
        User, Post, Comment, Tag,
        CreateUserInput, CreatePostInput, CreateCommentInput,
        CreateUserSuccess, CreatePostSuccess, CreateCommentSuccess,
        ValidationError, NotFoundError
    )

    # Import sample queries and mutations for testing
    @fraiseql.query
    async def users(info, limit: int = 10) -> list[User]:
        db = info.context["db"]
        users_data = await db.find("users", limit=limit, order_by="created_at DESC")
        return [User(**user) for user in users_data]

    @fraiseql.query
    async def posts(info, limit: int = 10) -> list[Post]:
        db = info.context["db"]
        posts_data = await db.find("posts", limit=limit, order_by="published_at DESC")
        return [Post(**post) for post in posts_data]

    # Create app with blog models
    app = fraiseql_app_factory(
        types=[User, Post, Comment, Tag],
        queries=[users, posts],
        mutations=[],  # Add mutations as needed
        database_url=postgres_url,
        production=False,
        enable_introspection=True,
        enable_playground=True
    )

    return app


@pytest_asyncio.fixture
async def integration_client(integration_app):
    """GraphQL client for integration testing."""
    from fastapi.testclient import TestClient
    from tests_new.fixtures.graphql import GraphQLTestClient

    with TestClient(integration_app) as client:
        yield GraphQLTestClient(client)


@pytest_asyncio.fixture
async def seeded_integration_db(db_connection, db_schema_setup, sample_blog_data):
    """Database seeded with sample blog data for integration tests."""
    # Insert users
    for user in sample_blog_data["users"]:
        await db_connection.execute("""
            INSERT INTO users (id, username, email, password_hash, role, profile, created_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s)
        """, (
            user["id"], user["username"], user["email"],
            "hashed_password", user["role"], Json(user.get("profile", {})),
            user["created_at"]
        ))

    # Insert categories/tags
    for category in sample_blog_data["categories"]:
        await db_connection.execute("""
            INSERT INTO tags (id, name, slug, description, color, created_at)
            VALUES (%s, %s, %s, %s, %s, %s)
        """, (
            category["id"], category["name"], category["slug"],
            category.get("description"), category.get("color"),
            category["created_at"]
        ))

    # Insert posts
    for post in sample_blog_data["posts"]:
        await db_connection.execute("""
            INSERT INTO posts (id, title, slug, content, excerpt, author_id, status, published_at, created_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
        """, (
            post["id"], post["title"], post["slug"], post["content"],
            post.get("excerpt"), post["author_id"], post["status"],
            post.get("published_at"), post["created_at"]
        ))

    # Insert comments
    for comment in sample_blog_data["comments"]:
        await db_connection.execute("""
            INSERT INTO comments (id, post_id, author_id, parent_id, content, status, created_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s)
        """, (
            comment["id"], comment["post_id"], comment["author_id"],
            comment.get("parent_id"), comment["content"], comment["status"],
            comment["created_at"]
        ))

    await db_connection.commit()

    yield sample_blog_data


@pytest.fixture
def performance_monitor():
    """Monitor for tracking performance metrics in integration tests."""
    import time
    from dataclasses import dataclass, field
    from typing import List

    @dataclass
    class PerformanceMetrics:
        query_times: List[float] = field(default_factory=list)
        mutation_times: List[float] = field(default_factory=list)
        connection_times: List[float] = field(default_factory=list)
        memory_usage: List[int] = field(default_factory=list)

        def add_query_time(self, duration: float):
            self.query_times.append(duration)

        def add_mutation_time(self, duration: float):
            self.mutation_times.append(duration)

        def get_average_query_time(self) -> float:
            return sum(self.query_times) / len(self.query_times) if self.query_times else 0.0

        def get_max_query_time(self) -> float:
            return max(self.query_times) if self.query_times else 0.0

        def assert_performance_acceptable(self, max_avg_query_time: float = 0.1):
            avg_time = self.get_average_query_time()
            assert avg_time <= max_avg_query_time, f"Average query time {avg_time}s exceeds limit {max_avg_query_time}s"

            max_time = self.get_max_query_time()
            assert max_time <= max_avg_query_time * 3, f"Max query time {max_time}s exceeds limit {max_avg_query_time * 3}s"

    return PerformanceMetrics()


@pytest.fixture
def integration_context_factory(db_connection, admin_user):
    """Factory for creating integration test GraphQL contexts."""

    def create_context(user=None, **extra):
        context = {
            "db": db_connection,
            "user": user or admin_user,
            "user_id": (user or admin_user)["id"],
            "request": None,
            **extra
        }
        return context

    return create_context


# Markers for integration test types
integration_database = pytest.mark.integration
integration_slow = pytest.mark.slow
integration_external = pytest.mark.skip(reason="Requires external services")


def pytest_collection_modifyitems(config, items):
    """Modify integration test collection."""
    # Add integration marker to all tests in integration directory
    for item in items:
        if "integration" in str(item.fspath):
            item.add_marker(pytest.mark.integration)

            # Add database marker if test uses database
            if any(fixture in item.fixturenames for fixture in ["db_connection", "integration_client", "seeded_integration_db"]):
                item.add_marker(pytest.mark.database)


@pytest.fixture(autouse=True)
def integration_test_isolation(request):
    """Ensure integration tests are properly isolated."""
    # Mark test start
    test_start_time = time.time()

    yield

    # Mark test end and check duration
    test_duration = time.time() - test_start_time

    # Warn about slow integration tests
    if test_duration > 5.0:  # 5 seconds
        print(f"\n⚠️  Slow integration test: {request.node.name} took {test_duration:.2f}s")


@pytest.fixture
def mock_external_service():
    """Mock external service for integration testing."""
    from unittest.mock import Mock, AsyncMock

    service = Mock()
    service.get_data = AsyncMock(return_value={"status": "ok", "data": "mock_data"})
    service.post_data = AsyncMock(return_value={"status": "created", "id": "mock_id"})
    service.is_available = AsyncMock(return_value=True)

    return service
