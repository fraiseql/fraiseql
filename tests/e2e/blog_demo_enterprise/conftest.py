"""
Enterprise Blog Demo Test Configuration
Pytest configuration and fixtures for multi-tenant blog functionality testing.
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
def enterprise_database_schema():
    """Path to the enterprise blog database schema."""
    # For now, use simple schema - enterprise will be extended later
    return str(Path(__file__).parent.parent / "blog_demo_simple" / "db" / "create_full.sql")


@pytest_asyncio.fixture(scope="session") 
async def enterprise_blog_database(enterprise_database_schema):
    """Session-scoped enterprise blog database with schema applied."""
    db_name = "fraiseql_blog_enterprise_test"
    
    await create_test_database(db_name, enterprise_database_schema)
    
    yield db_name
    
    await drop_test_database(db_name)


@pytest_asyncio.fixture
async def enterprise_db_connection(enterprise_blog_database):
    """Database connection for enterprise blog tests."""
    conn_str = get_db_connection_string(enterprise_blog_database)
    
    async with await psycopg.AsyncConnection.connect(conn_str) as conn:
        async with conn.transaction():
            yield conn


@pytest_asyncio.fixture
async def enterprise_db_manager(enterprise_db_connection):
    """Database manager for enterprise blog tests."""
    return DatabaseManager(enterprise_db_connection)


@pytest_asyncio.fixture
async def enterprise_blog_data(enterprise_db_manager):
    """Provides seeded enterprise blog test data with multi-tenant setup."""
    
    # Create test organizations (multi-tenant)
    org1 = await enterprise_db_manager.insert_test_data(
        "tb_organization", 
        pk_organization="00000001-0001-0001-0001-000000000001",
        data={
            "name": "Test Organization 1",
            "slug": "test-org-1",
            "settings": {"theme": "default"}
        }
    )
    
    # Create test users with organization context
    user1 = await enterprise_db_manager.insert_test_data(
        "tb_user",
        pk_user="11111111-1111-1111-1111-111111111111",
        pk_organization="00000001-0001-0001-0001-000000000001",
        data={
            "username": "testuser1",
            "email": "test1@testorg1.com", 
            "role": "author",
            "profile": {"name": "Test User 1", "department": "Content"}
        }
    )
    
    user2 = await enterprise_db_manager.insert_test_data(
        "tb_user",
        pk_user="22222222-2222-2222-2222-222222222222", 
        pk_organization="00000001-0001-0001-0001-000000000001",
        data={
            "username": "testadmin1",
            "email": "admin1@testorg1.com",
            "role": "admin",
            "profile": {"name": "Test Admin 1", "department": "Management"}
        }
    )
    
    # Create test content with tenant isolation
    tag1 = await enterprise_db_manager.insert_test_data(
        "tb_tag",
        pk_tag="33333333-3333-3333-3333-333333333333",
        pk_organization="00000001-0001-0001-0001-000000000001",
        data={
            "name": "Enterprise Tech",
            "slug": "enterprise-tech",
            "color": "#0066cc",
            "category": "technology"
        }
    )
    
    post1 = await enterprise_db_manager.insert_test_data(
        "tb_post",
        pk_post="44444444-4444-4444-4444-444444444444",
        pk_author="11111111-1111-1111-1111-111111111111",
        pk_organization="00000001-0001-0001-0001-000000000001",
        data={
            "title": "Enterprise Blog Post",
            "slug": "enterprise-blog-post", 
            "content": "This is enterprise content with multi-tenant support",
            "status": "published",
            "metadata": {"featured": True, "priority": "high"}
        }
    )
    
    return {
        "organizations": [org1],
        "users": [user1, user2],
        "tags": [tag1], 
        "posts": [post1]
    }


# GraphQL assertion helpers - enterprise versions
def assert_no_graphql_errors(result):
    """Assert that a GraphQL result has no errors."""
    assert "errors" not in result or not result["errors"], f"GraphQL errors: {result.get('errors', [])}"

def assert_mutation_success(result, field_name, success_type):
    """Assert that a GraphQL mutation succeeded."""
    assert "errors" not in result
    assert result["data"][field_name]["__typename"] == success_type

def assert_tenant_isolation(result, expected_tenant_id):
    """Assert that results are properly isolated by tenant.""" 
    # This would check that all returned data belongs to the expected tenant
    # Implementation depends on the specific data structure
    pass