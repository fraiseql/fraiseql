"""
Test fixtures for GraphQL Cascade functionality.

Provides test app, client, and database setup for cascade integration tests.
"""

import pytest
import asyncio
from typing import AsyncGenerator
from unittest.mock import AsyncMock, MagicMock

import pytest_asyncio
from fastapi import FastAPI
from fastapi.testclient import TestClient
from httpx import AsyncClient, ASGITransport

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.mutations import mutation


# Test types for cascade
@fraiseql.input
class CreatePostInput:
    title: str
    content: str = None
    author_id: str


@fraiseql.type
class Post:
    id: str
    title: str
    content: str = None
    author_id: str


@fraiseql.type
class User:
    id: str
    name: str
    post_count: int


@fraiseql.type
class CreatePostSuccess:
    id: str
    message: str


@fraiseql.type
class CreatePostError:
    code: str
    message: str


# Test mutations
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError


@pytest_asyncio.fixture
async def cascade_app() -> AsyncGenerator[FastAPI, None]:
    """FastAPI app configured with cascade mutations."""
    app = create_fraiseql_app(
        types=[CreatePostInput, Post, User, CreatePostSuccess, CreatePostError],
        database_url="postgresql://test:test@localhost:5432/test_db",  # Will be mocked
    )

    yield app


@pytest_asyncio.fixture
async def cascade_client(cascade_app: FastAPI) -> AsyncGenerator[TestClient, None]:
    """Test client for cascade app."""
    with TestClient(cascade_app) as client:
        yield client


@pytest_asyncio.fixture
async def cascade_http_client(cascade_app: FastAPI) -> AsyncGenerator[AsyncClient, None]:
    """Async HTTP client for cascade app."""
    async with cascade_app.router.lifespan_context(cascade_app):
        transport = ASGITransport(app=cascade_app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            yield client


@pytest.fixture
def mock_apollo_client():
    """Mock Apollo Client for cascade integration testing."""
    client = MagicMock()
    client.cache = MagicMock()
    client.cache.writeFragment = AsyncMock()
    client.cache.evict = AsyncMock()
    client.cache.identify = MagicMock(return_value="Post:123")
    return client
