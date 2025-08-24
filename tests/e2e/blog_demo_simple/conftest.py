"""
Simple Blog Demo Test Configuration
Pytest configuration and fixtures for basic blog functionality testing.
"""

import sys
from pathlib import Path

import pytest
import pytest_asyncio

# Import our database fixtures
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "fixtures"))
from database.setup import create_test_database, drop_test_database, get_db_connection_string, DatabaseManager
import psycopg


@pytest.fixture(scope="session")
def simple_database_schema():
    """Path to the simple blog database schema."""
    return str(Path(__file__).parent / "db" / "create_full.sql")


@pytest_asyncio.fixture(scope="session") 
async def simple_blog_database(simple_database_schema):
    """Session-scoped simple blog database with schema applied."""
    db_name = "fraiseql_blog_simple_test"
    
    await create_test_database(db_name, simple_database_schema)
    
    yield db_name
    
    await drop_test_database(db_name)


@pytest_asyncio.fixture
async def simple_db_connection(simple_blog_database):
    """Database connection for simple blog tests."""
    conn_str = get_db_connection_string(simple_blog_database)
    
    async with await psycopg.AsyncConnection.connect(conn_str) as conn:
        async with conn.transaction():
            yield conn


@pytest_asyncio.fixture
async def simple_db_manager(simple_db_connection):
    """Database manager for simple blog tests."""
    return DatabaseManager(simple_db_connection)


@pytest_asyncio.fixture
async def simple_blog_data(simple_db_manager):
    """Provides seeded simple blog test data."""
    
    # Create test users
    user1 = await simple_db_manager.insert_test_data(
        "tb_user",
        pk_user="11111111-1111-1111-1111-111111111111",
        data={
            "username": "testuser1",
            "email": "test1@example.com", 
            "role": "author",
            "profile": {"name": "Test User 1"}
        }
    )
    
    # Create test tags
    tag1 = await simple_db_manager.insert_test_data(
        "tb_tag",
        pk_tag="33333333-3333-3333-3333-333333333333",
        data={
            "name": "technology",
            "slug": "technology",
            "color": "#0066cc"
        }
    )
    
    # Create test post
    post1 = await simple_db_manager.insert_test_data(
        "tb_post",
        pk_post="44444444-4444-4444-4444-444444444444",
        pk_author="11111111-1111-1111-1111-111111111111",
        data={
            "title": "Test Post 1",
            "slug": "test-post-1", 
            "content": "This is test content",
            "status": "published"
        }
    )
    
    return {
        "users": [user1],
        "tags": [tag1], 
        "posts": [post1]
    }


# GraphQL assertion helpers - simple implementations
def assert_no_graphql_errors(result):
    """Assert that a GraphQL result has no errors."""
    assert "errors" not in result or not result["errors"], f"GraphQL errors: {result.get('errors', [])}"

def assert_mutation_success(result, field_name, success_type):
    """Assert that a GraphQL mutation succeeded."""
    assert "errors" not in result
    assert result["data"][field_name]["__typename"] == success_type

def assert_graphql_field_equals(result, field_path, expected_value):
    """Assert that a GraphQL field equals expected value."""
    keys = field_path.split(".")
    current = result["data"]
    for key in keys:
        current = current[key]
    assert current == expected_value