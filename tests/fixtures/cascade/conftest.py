"""
Test fixtures for GraphQL Cascade functionality.

Provides test app, client, and database setup for cascade integration tests.
"""

from typing import Optional
from unittest.mock import AsyncMock, MagicMock

import pytest
import pytest_asyncio
from fastapi import FastAPI
from fastapi.testclient import TestClient
from httpx import ASGITransport, AsyncClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.mutations import mutation


# Test types for cascade
@fraiseql.input
class CreatePostInput:
    title: str
    content: Optional[str] = None
    author_id: str


@fraiseql.type
class Post:
    id: str
    title: str
    content: Optional[str] = None
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
async def cascade_db_schema(db_pool):
    """Set up cascade test database schema with tables and PostgreSQL function.

    Uses the shared db_pool fixture from database_conftest.py for proper database access.
    Creates tables and a PostgreSQL function that returns cascade data.
    """
    async with db_pool.connection() as conn:
        # Create tables
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS tb_user (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                post_count INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS tb_post (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT,
                author_id TEXT REFERENCES tb_user(id)
            );

            -- PostgreSQL function for cascade mutation
            -- Takes JSONB input (as per FraiseQL mutation convention)
            CREATE OR REPLACE FUNCTION create_post(input_data JSONB)
            RETURNS JSONB AS $$
            DECLARE
                p_title TEXT;
                p_content TEXT;
                p_author_id TEXT;
                v_post_id TEXT;
                v_cascade JSONB;
            BEGIN
                -- Extract input parameters (camelCase from GraphQL)
                p_title := input_data->>'title';
                p_content := input_data->>'content';
                p_author_id := input_data->>'authorId';

                -- Validate input
                IF p_title = '' OR p_title IS NULL THEN
                    RETURN jsonb_build_object(
                        'code', 'VALIDATION_ERROR',
                        'message', 'Title cannot be empty'
                    );
                END IF;

                -- Check if user exists
                IF NOT EXISTS (SELECT 1 FROM tb_user WHERE id = p_author_id) THEN
                    RETURN jsonb_build_object(
                        'code', 'VALIDATION_ERROR',
                        'message', 'Author not found'
                    );
                END IF;

                -- Create post
                v_post_id := 'post-' || gen_random_uuid()::text;

                INSERT INTO tb_post (id, title, content, author_id)
                VALUES (v_post_id, p_title, p_content, p_author_id);

                -- Update user post count
                UPDATE tb_user
                SET post_count = post_count + 1
                WHERE id = p_author_id;

                -- Build cascade data (using camelCase for GraphQL)
                v_cascade := jsonb_build_object(
                    'updated', jsonb_build_array(
                        jsonb_build_object(
                            '__typename', 'Post',
                            'id', v_post_id,
                            'operation', 'CREATED',
                            'entity', jsonb_build_object(
                                'id', v_post_id,
                                'title', p_title,
                                'content', p_content,
                                'authorId', p_author_id
                            )
                        ),
                        jsonb_build_object(
                            '__typename', 'User',
                            'id', p_author_id,
                            'operation', 'UPDATED',
                            'entity', (
                                SELECT jsonb_build_object(
                                    'id', id,
                                    'name', name,
                                    'postCount', post_count
                                )
                                FROM tb_user WHERE id = p_author_id
                            )
                        )
                    ),
                    'deleted', jsonb_build_array(),
                    'invalidations', jsonb_build_array(
                        jsonb_build_object(
                            'queryName', 'posts',
                            'strategy', 'INVALIDATE',
                            'scope', 'PREFIX'
                        )
                    ),
                    'metadata', jsonb_build_object(
                        'timestamp', NOW()::text,
                        'affectedCount', 2
                    )
                );

                -- Return success with cascade (note the _cascade field)
                RETURN jsonb_build_object(
                    'id', v_post_id,
                    'message', 'Post created successfully',
                    '_cascade', v_cascade
                );
            END;
            $$ LANGUAGE plpgsql;

            -- Insert test user
            INSERT INTO tb_user (id, name, post_count)
            VALUES ('user-123', 'Test User', 0)
            ON CONFLICT (id) DO NOTHING;
        """)
        await conn.commit()

    yield

    # Cleanup
    async with db_pool.connection() as conn:
        await conn.execute("""
            DROP FUNCTION IF EXISTS create_post(JSONB);
            DROP TABLE IF EXISTS tb_post CASCADE;
            DROP TABLE IF EXISTS tb_user CASCADE;
        """)
        await conn.commit()


@pytest.fixture
def cascade_app(cascade_db_schema, postgres_url) -> FastAPI:
    """FastAPI app configured with cascade mutations.

    Uses the real postgres_url from database fixtures.
    Depends on cascade_db_schema to ensure schema is set up.
    """
    app = create_fraiseql_app(
        types=[CreatePostInput, Post, User, CreatePostSuccess, CreatePostError],
        mutations=[CreatePost],
        database_url=postgres_url,
    )
    return app


@pytest.fixture
def cascade_client(cascade_app: FastAPI) -> TestClient:
    """Test client for cascade app (synchronous client for simple tests)."""
    return TestClient(cascade_app)


@pytest_asyncio.fixture
async def cascade_http_client(cascade_app: FastAPI) -> AsyncClient:
    """Async HTTP client for cascade app (for async test scenarios)."""
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
