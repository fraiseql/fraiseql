"""Test configuration for Blog E2E Test Suite.

Provides database setup, GraphQL client, and fixtures for testing the complete
blog application with database-first architecture patterns.
"""

import asyncio
import os
import tempfile
from pathlib import Path
from typing import AsyncGenerator

import asyncpg
import pytest
import pytest_asyncio
from httpx import AsyncClient

from fraiseql import FraiseQL
from fraiseql.cqrs import CQRSRepository


# Test database configuration
TEST_DB_CONFIG = {
    "host": "localhost",
    "port": 5432,
    "user": "postgres", 
    "password": "postgres",
    "database": "blog_e2e_test"
}

# Test UUIDs following PrintOptim patterns
TEST_SYSTEM_USER_ID = "11111111-1111-1111-1111-111111111111"
TEST_AUTHOR_ID = "22222222-2222-2222-2222-222222222222"
TEST_ADMIN_USER_ID = "33333333-3333-3333-3333-333333333333"


@pytest_asyncio.fixture(scope="session")
async def database_url():
    """Provide database connection URL for testing."""
    return f"postgresql://{TEST_DB_CONFIG['user']}:{TEST_DB_CONFIG['password']}@{TEST_DB_CONFIG['host']}:{TEST_DB_CONFIG['port']}/{TEST_DB_CONFIG['database']}"


@pytest_asyncio.fixture(scope="session") 
async def setup_test_database(database_url):
    """Setup and teardown test database with schema."""
    # Connect to postgres database to create test database
    postgres_url = database_url.replace("/blog_e2e_test", "/postgres")
    
    conn = await asyncpg.connect(postgres_url)
    
    # Drop and recreate test database
    await conn.execute("DROP DATABASE IF EXISTS blog_e2e_test")
    await conn.execute("CREATE DATABASE blog_e2e_test")
    await conn.close()
    
    # Connect to test database and load schema
    conn = await asyncpg.connect(database_url)
    
    # Load schema from file
    schema_path = Path(__file__).parent / "schema.sql"
    schema_sql = schema_path.read_text()
    await conn.execute(schema_sql)
    
    await conn.close()
    
    yield database_url
    
    # Cleanup
    conn = await asyncpg.connect(postgres_url)
    await conn.execute("DROP DATABASE IF EXISTS blog_e2e_test")
    await conn.close()


@pytest_asyncio.fixture
async def db_connection(setup_test_database, database_url):
    """Provide database connection for individual tests."""
    conn = await asyncpg.connect(database_url)
    yield conn
    await conn.close()


@pytest_asyncio.fixture  
async def fraiseql_app(db_connection):
    """Create FraiseQL application with blog schema."""
    app = FraiseQL()
    
    # Load database functions
    functions_path = Path(__file__).parent / "functions.sql"
    functions_sql = functions_path.read_text()
    await db_connection.execute(functions_sql)
    
    # Add database connection to context
    async def get_context():
        return {
            "db": CQRSRepository(db_connection),
            "user_id": TEST_SYSTEM_USER_ID,
            "current_user_id": TEST_SYSTEM_USER_ID
        }
    
    app.context_getter = get_context
    
    # Import and register GraphQL types and mutations
    from . import graphql_types
    
    # Register all the mutation classes (they self-register via BlogMutationBase)
    app.add_type(graphql_types.CreateAuthor)
    app.add_type(graphql_types.CreatePost) 
    app.add_type(graphql_types.CreateTag)
    app.add_type(graphql_types.CreateComment)
    
    # Register input/output types
    app.add_type(graphql_types.CreateAuthorInput)
    app.add_type(graphql_types.CreateAuthorSuccess)
    app.add_type(graphql_types.CreateAuthorError)
    app.add_type(graphql_types.Author)
    
    app.add_type(graphql_types.CreatePostInput)
    app.add_type(graphql_types.CreatePostSuccess)
    app.add_type(graphql_types.CreatePostError)
    app.add_type(graphql_types.Post)
    
    app.add_type(graphql_types.CreateTagInput)
    app.add_type(graphql_types.CreateTagSuccess)
    app.add_type(graphql_types.CreateTagError)
    app.add_type(graphql_types.Tag)
    
    app.add_type(graphql_types.CreateCommentInput)
    app.add_type(graphql_types.CreateCommentSuccess)
    app.add_type(graphql_types.CreateCommentError)
    app.add_type(graphql_types.Comment)
    
    return app


@pytest_asyncio.fixture
async def http_client(fraiseql_app):
    """HTTP client for GraphQL API testing."""
    from fastapi import FastAPI
    from fastapi.middleware.cors import CORSMiddleware
    
    # Create FastAPI app
    fastapi_app = FastAPI(title="Blog E2E Test API")
    
    # Add CORS middleware
    fastapi_app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )
    
    # Mount FraiseQL
    fraiseql_app.mount_fastapi(fastapi_app, path="/graphql")
    
    # Create test client
    async with AsyncClient(app=fastapi_app, base_url="http://testserver") as client:
        yield client


@pytest_asyncio.fixture
async def graphql_client(http_client):
    """GraphQL test client for executing queries and mutations."""
    
    class BlogGraphQLClient:
        """GraphQL client for blog API testing."""
        
        def __init__(self, http_client: AsyncClient):
            self.http_client = http_client
        
        async def execute(self, query: str, variables: dict = None) -> dict:
            """Execute GraphQL query/mutation and return result."""
            payload = {"query": query}
            if variables:
                payload["variables"] = variables
            
            response = await self.http_client.post("/graphql", json=payload)
            return response.json()
        
        async def create_author(self, **kwargs) -> dict:
            """Helper method to create author via GraphQL."""
            mutation = """
                mutation CreateAuthor($input: CreateAuthorInput!) {
                    createAuthor(input: $input) {
                        __typename
                        ... on CreateAuthorSuccess {
                            author {
                                id
                                identifier
                                name
                                email
                            }
                            message
                        }
                        ... on CreateAuthorError {
                            message
                            errorCode
                            originalPayload
                            conflictAuthor {
                                id
                                identifier
                                name
                            }
                        }
                    }
                }
            """
            
            # Set defaults for test data
            defaults = {
                "identifier": "test-author",
                "name": "Test Author",  
                "email": "test@example.com",
                "bio": "A test author for E2E testing"
            }
            defaults.update(kwargs)
            
            return await self.execute(mutation, {"input": defaults})
        
        async def create_post(self, **kwargs) -> dict:
            """Helper method to create blog post via GraphQL."""
            mutation = """
                mutation CreatePost($input: CreatePostInput!) {
                    createPost(input: $input) {
                        __typename
                        ... on CreatePostSuccess {
                            post {
                                id
                                identifier
                                title
                                status
                                authorId
                                tags
                            }
                            message
                        }
                        ... on CreatePostError {
                            message
                            errorCode
                            originalPayload
                            conflictPost {
                                id
                                identifier
                                title
                            }
                            missingAuthor {
                                identifier
                            }
                            invalidTags
                        }
                    }
                }
            """
            
            # Set defaults for test data
            defaults = {
                "identifier": "test-post",
                "title": "Test Blog Post",
                "content": "This is a test blog post for E2E testing",
                "authorIdentifier": "test-author",
                "status": "draft"
            }
            defaults.update(kwargs)
            
            return await self.execute(mutation, {"input": defaults})
    
    return BlogGraphQLClient(http_client)


@pytest_asyncio.fixture
async def sample_author(db_connection):
    """Create a sample author directly in database for testing."""
    author_id = TEST_AUTHOR_ID
    author_data = {
        "name": "Sample Author",
        "email": "sample@example.com", 
        "bio": "A sample author for testing"
    }
    
    await db_connection.execute("""
        INSERT INTO blog.tb_author (pk_author, identifier, data, created_by, updated_by)
        VALUES ($1, $2, $3, $4, $5)
    """, author_id, "sample-author", author_data, TEST_SYSTEM_USER_ID, TEST_SYSTEM_USER_ID)
    
    yield {
        "id": author_id,
        "identifier": "sample-author", 
        "data": author_data
    }
    
    # Cleanup
    await db_connection.execute("DELETE FROM blog.tb_author WHERE pk_author = $1", author_id)


@pytest_asyncio.fixture
async def clean_database(db_connection):
    """Ensure clean database state before each test."""
    # Clean all tables in dependency order
    await db_connection.execute("DELETE FROM blog.tb_comment")
    await db_connection.execute("DELETE FROM blog.tb_post_tag")
    await db_connection.execute("DELETE FROM blog.tb_post")
    await db_connection.execute("DELETE FROM blog.tb_tag")
    await db_connection.execute("DELETE FROM blog.tb_author")
    
    # Clean materialized tables
    await db_connection.execute("DELETE FROM tv_author")
    await db_connection.execute("DELETE FROM tv_post") 
    await db_connection.execute("DELETE FROM tv_tag")
    await db_connection.execute("DELETE FROM tv_comment")
    
    yield
    
    # Cleanup after test (optional, but good practice)
    await db_connection.execute("DELETE FROM blog.tb_comment")
    await db_connection.execute("DELETE FROM blog.tb_post_tag") 
    await db_connection.execute("DELETE FROM blog.tb_post")
    await db_connection.execute("DELETE FROM blog.tb_tag")
    await db_connection.execute("DELETE FROM blog.tb_author")