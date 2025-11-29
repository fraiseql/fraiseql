"""Test fixtures for GraphQL Cascade functionality.

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
@mutation(enable_cascade=True, function="create_post")
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError


# Test query (required for GraphQL schema)
from graphql import GraphQLResolveInfo


async def get_post(info: GraphQLResolveInfo, id: str) -> Optional[Post]:
    """Simple query to satisfy GraphQL schema requirements."""
    return None  # Not needed for cascade tests


@pytest_asyncio.fixture
async def cascade_db_schema(db_pool):
    """Set up cascade test database schema with tables and PostgreSQL function.

    Uses the shared db_pool fixture from database_conftest.py for proper database access.
    Creates tables and a PostgreSQL function that returns mutation_result_v2 with cascade data.
    """
    async with db_pool.connection() as conn:
        # Create mutation_result_v2 type (from migrations/trinity/005_add_mutation_result_v2.sql)
        await conn.execute("""
            DO $$ BEGIN
                CREATE TYPE mutation_result_v2 AS (
                    status          text,
                    message         text,
                    entity_id       text,
                    entity_type     text,
                    entity          jsonb,
                    updated_fields  text[],
                    cascade         jsonb,
                    metadata        jsonb
                );
            EXCEPTION
                WHEN duplicate_object THEN null;
            END $$;
        """)

        # Create tables in public schema explicitly
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS public.tb_user (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                post_count INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS public.tb_post (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT,
                author_id TEXT REFERENCES public.tb_user(id)
            );
        """)

        # Create PostgreSQL function returning mutation_result_v2
        # FraiseQL's executor wraps with row_to_json() which works with composite types
        await conn.execute("""
            CREATE OR REPLACE FUNCTION public.create_post(input_data JSONB)
            RETURNS mutation_result_v2 AS $$
            DECLARE
                p_title TEXT;
                p_content TEXT;
                p_author_id TEXT;
                v_post_id TEXT;
                v_cascade JSONB;
                v_entity JSONB;
            BEGIN
                -- Extract input parameters (snake_case from FraiseQL)
                p_title := input_data->>'title';
                p_content := input_data->>'content';
                p_author_id := input_data->>'author_id';

                -- Validate input
                IF p_title = '' OR p_title IS NULL THEN
                    RETURN ROW(
                        'failed:validation',
                        'Title cannot be empty',
                        NULL, NULL, NULL, NULL, NULL,
                        jsonb_build_object('field', 'title')
                    )::mutation_result_v2;
                END IF;

                -- Check if user exists
                IF NOT EXISTS (SELECT 1 FROM public.tb_user WHERE id = p_author_id) THEN
                    RETURN ROW(
                        'failed:not_found',
                        'Author not found',
                        NULL, NULL, NULL, NULL, NULL,
                        jsonb_build_object('resource', 'User', 'id', p_author_id)
                    )::mutation_result_v2;
                END IF;

                -- Create post
                v_post_id := 'post-' || gen_random_uuid()::text;

                INSERT INTO public.tb_post (id, title, content, author_id)
                VALUES (v_post_id, p_title, p_content, p_author_id);

                -- Update user post count
                UPDATE public.tb_user
                SET post_count = post_count + 1
                WHERE id = p_author_id;

                -- Build entity data
                v_entity := jsonb_build_object(
                    'id', v_post_id,
                    'title', p_title,
                    'content', p_content,
                    'author_id', p_author_id
                );

                -- Build cascade data per GraphQL Cascade spec
                -- Use camelCase for cascade fields (passed through as-is)
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
                                FROM public.tb_user WHERE id = p_author_id
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

                -- Return success with cascade via mutation_result_v2
                RETURN ROW(
                    'new',
                    'Post created successfully',
                    v_post_id,
                    'Post',
                    v_entity,
                    NULL::text[],
                    v_cascade,
                    NULL::jsonb
                )::mutation_result_v2;
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Insert test user in a separate statement
        await conn.execute("""
            INSERT INTO public.tb_user (id, name, post_count)
            VALUES ('user-123', 'Test User', 0)
            ON CONFLICT (id) DO NOTHING;
        """)
        await conn.commit()

    yield

    # Note: We intentionally skip cleanup here because:
    # 1. The tables/functions are created with IF NOT EXISTS/CREATE OR REPLACE
    # 2. The session-scoped pool may be closed before this function-scoped fixture tears down
    # 3. The test database is ephemeral (testcontainer) so cleanup is not necessary
    # This avoids async event loop issues during fixture teardown


@pytest.fixture
def cascade_app(cascade_db_schema, create_fraiseql_app_with_db) -> FastAPI:
    """FastAPI app configured with cascade mutations.

    Uses create_fraiseql_app_with_db for shared database pool.
    Depends on cascade_db_schema to ensure schema is set up.
    """
    app = create_fraiseql_app_with_db(
        types=[CreatePostInput, Post, User, CreatePostSuccess, CreatePostError],
        queries=[get_post],
        mutations=[CreatePost],
    )
    return app


@pytest.fixture
def cascade_client(cascade_app: FastAPI) -> TestClient:
    """Test client for cascade app (synchronous client for simple tests).

    Note: Uses raise_server_exceptions=False to avoid event loop conflicts
    during teardown when mixing async and sync fixtures.
    """
    with TestClient(cascade_app, raise_server_exceptions=False) as client:
        yield client


@pytest_asyncio.fixture
async def cascade_http_client(cascade_app: FastAPI) -> AsyncClient:
    """Async HTTP client for cascade app (for async test scenarios).

    Uses LifespanManager to properly trigger ASGI lifespan events.
    """
    from asgi_lifespan import LifespanManager

    async with LifespanManager(cascade_app) as manager:
        transport = ASGITransport(app=manager.app)
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
