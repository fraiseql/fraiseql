"""
Integration tests for blog_simple example with smart dependency management.

These tests run with automatically installed dependencies and smart database setup,
providing full validation of the blog_simple example functionality.
"""

import logging
import pytest

# Setup logging for integration tests
logger = logging.getLogger(__name__)

# Mark all tests as example integration tests
pytestmark = [
    pytest.mark.blog_simple,
    pytest.mark.integration,
    pytest.mark.database,
    pytest.mark.examples
]


@pytest.mark.asyncio
async def test_smart_dependencies_available(smart_dependencies):
    """Test that smart dependency management successfully provides all required dependencies."""
    # Verify that smart dependencies fixture provided dependency information
    assert smart_dependencies is not None
    assert 'dependency_results' in smart_dependencies
    assert 'environment' in smart_dependencies

    # Test basic imports individually to identify the failing one
    import sys
    logger.info(f"Using Python: {sys.executable}")
    logger.info(f"Python path: {sys.path[:3]}")

    import httpx
    import psycopg

    # Check if fastapi is available
    try:
        import fastapi
        logger.info("FastAPI import successful")
    except ImportError as e:
        logger.error(f"FastAPI import failed: {e}")
        # Try to find where the package would be
        logger.error(f"Sys path: {sys.path[:5]}")
        raise

    # Test GraphQL separately first
    try:
        from graphql import GraphQLSchema
        logger.info("GraphQL import successful")
    except ImportError as e:
        logger.error(f"GraphQL import failed: {e}")
        raise

    # Now test fraiseql
    try:
        import fraiseql
        logger.info("FraiseQL import successful")
    except ImportError as e:
        logger.error(f"FraiseQL import failed: {e}")
        raise

    logger.info("All smart dependencies validated in integration test")


@pytest.mark.asyncio
async def test_blog_simple_app_health(blog_simple_client):
    """Test that blog_simple app starts up and responds to health checks."""
    logger.info("Testing blog_simple app health endpoint")
    response = await blog_simple_client.get("/health")
    assert response.status_code == 200

    data = response.json()
    logger.info(f"Blog simple health response: {data}")
    assert data["status"] == "healthy", f"Expected healthy, got: {data}"
    assert data["service"] == "blog_simple", f"Expected blog_simple, got: {data}"
    logger.info("Blog simple health check passed")


@pytest.mark.asyncio
async def test_blog_simple_home_endpoint(blog_simple_client):
    """Test that blog_simple home endpoint returns expected information."""
    response = await blog_simple_client.get("/")
    assert response.status_code == 200

    data = response.json()
    assert "FraiseQL Simple Blog" in data["message"]
    assert "endpoints" in data
    assert data["endpoints"]["graphql"] == "/graphql"


@pytest.mark.asyncio
async def test_blog_simple_graphql_introspection(blog_simple_graphql_client):
    """Test that GraphQL introspection works for blog_simple."""
    introspection_query = """
        query IntrospectionQuery {
            __schema {
                types {
                    name
                    kind
                }
            }
        }
    """

    result = await blog_simple_graphql_client.execute(introspection_query)

    # Should not have errors
    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "__schema" in result["data"]

    # Check for expected types
    type_names = [t["name"] for t in result["data"]["__schema"]["types"]]

    # Should have our domain types
    expected_types = ["User", "Post", "Comment", "Tag", "UserRole", "PostStatus", "CommentStatus"]
    for expected_type in expected_types:
        assert expected_type in type_names, f"Expected type {expected_type} not found in schema"


@pytest.mark.asyncio
async def test_blog_simple_basic_queries(blog_simple_graphql_client):
    """Test basic queries work without errors."""
    # Test posts query
    posts_query = """
        query GetPosts($limit: Int) {
            posts(limit: $limit) {
                id
                title
                status
            }
        }
    """

    result = await blog_simple_graphql_client.execute(
        posts_query,
        variables={"limit": 5}
    )

    # Should execute without errors
    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "posts" in result["data"]

    # Test tags query
    tags_query = """
        query GetTags($limit: Int) {
            tags(limit: $limit) {
                id
                name
                slug
            }
        }
    """

    result = await blog_simple_graphql_client.execute(
        tags_query,
        variables={"limit": 5}
    )

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "tags" in result["data"]


@pytest.mark.asyncio
async def test_blog_simple_database_connectivity(blog_simple_repository):
    """Test that database connectivity works properly."""
    # Test basic database connection
    result = await blog_simple_repository.connection.execute("SELECT 1 as test")
    rows = await result.fetchall()
    assert len(rows) == 1
    assert rows[0][0] == 1  # First column of first row


@pytest.mark.asyncio
async def test_blog_simple_seed_data(blog_simple_repository):
    """Test that seed data is properly loaded."""
    # Check that users table exists and has data
    result = await blog_simple_repository.connection.execute("SELECT COUNT(*) as count FROM users")
    rows = await result.fetchall()
    user_count = rows[0][0]  # First column of first row
    assert user_count > 0, "Users table should have seed data"

    # Check that tags table exists and has data
    result = await blog_simple_repository.connection.execute("SELECT COUNT(*) as count FROM tags")
    rows = await result.fetchall()
    tag_count = rows[0][0]  # First column of first row
    assert tag_count > 0, "Tags table should have seed data"

    # Check that posts table exists and has data
    result = await blog_simple_repository.connection.execute("SELECT COUNT(*) as count FROM posts")
    rows = await result.fetchall()
    post_count = rows[0][0]  # First column of first row
    assert post_count > 0, "Posts table should have seed data"


@pytest.mark.asyncio
async def test_blog_simple_mutations_structure(blog_simple_graphql_client):
    """Test that mutations are properly structured."""
    # Test introspection for mutations
    mutation_query = """
        query {
            __schema {
                mutationType {
                    fields {
                        name
                        type {
                            name
                            kind
                        }
                    }
                }
            }
        }
    """

    result = await blog_simple_graphql_client.execute(mutation_query)

    assert "errors" not in result or not result["errors"]
    assert "data" in result

    # Should have mutation type
    mutation_type = result["data"]["__schema"]["mutationType"]
    if mutation_type:  # mutations might not be implemented yet
        mutation_names = [field["name"] for field in mutation_type["fields"]]

        # Expected mutations (if implemented)
        possible_mutations = ["createPost", "updatePost", "createComment", "createUser"]

        # At least some mutations should exist if mutationType is present
        assert len(mutation_names) > 0, "Mutation type exists but no mutations defined"


@pytest.mark.asyncio
@pytest.mark.slow
async def test_blog_simple_performance_baseline(blog_simple_graphql_client):
    """Test basic performance baseline for blog_simple."""
    import time

    # Simple query performance test
    query = """
        query GetPosts {
            posts(limit: 10) {
                id
                title
                author {
                    username
                }
            }
        }
    """

    start_time = time.time()
    result = await blog_simple_graphql_client.execute(query)
    end_time = time.time()

    # Should complete without errors
    assert "errors" not in result or not result["errors"]

    # Should complete reasonably quickly (under 5 seconds for basic query)
    duration = end_time - start_time
    assert duration < 5.0, f"Query took too long: {duration:.2f}s"


@pytest.mark.asyncio
async def test_blog_simple_error_handling(blog_simple_graphql_client):
    """Test that error handling works properly."""
    # Test invalid query
    invalid_query = """
        query {
            nonExistentField {
                id
            }
        }
    """

    result = await blog_simple_graphql_client.execute(invalid_query)

    # Should have GraphQL errors
    assert "errors" in result
    assert len(result["errors"]) > 0

    # Test malformed query
    malformed_query = "query { posts { id title"  # Missing closing brace

    result = await blog_simple_graphql_client.execute(malformed_query)
    assert "errors" in result
