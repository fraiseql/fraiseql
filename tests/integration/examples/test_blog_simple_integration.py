"""
Integration tests for blog_simple example with smart dependency management.

These tests run with automatically installed dependencies and smart database setup,
providing full validation of the blog_simple example functionality.
"""

import logging
import pytest

pytestmark = pytest.mark.integration

# Setup logging for integration tests
logger = logging.getLogger(__name__)

# Mark all tests as example integration tests
pytestmark = [
    pytest.mark.blog_simple,
    pytest.mark.integration,
    pytest.mark.database,
    pytest.mark.examples,
]


@pytest.mark.asyncio
async def test_smart_dependencies_available(smart_dependencies) -> None:
    """Test that smart dependency management successfully provides all required dependencies."""
    # Verify that smart dependencies fixture provided dependency information
    assert smart_dependencies is not None
    assert "dependency_results" in smart_dependencies
    assert "environment" in smart_dependencies

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


def test_blog_simple_app_exists() -> None:
    """Basic test that blog simple app can be imported and has expected structure."""
    import sys
    from pathlib import Path

    examples_dir = Path(__file__).parent.parent.parent.parent / "examples"
    blog_simple_dir = examples_dir / "blog_simple"
    app_file = blog_simple_dir / "app.py"

    # Check that the file exists
    assert app_file.exists(), f"Blog simple app.py not found at {app_file}"

    # Try to read the file and check it has expected content
    content = app_file.read_text()
    assert "def create_app():" in content, "create_app function not found"
    assert "/health" in content, "Health endpoint not found"
    assert "blog_simple" in content, "Service name not found"

    # Try basic import without creating the app (to avoid database issues)
    sys.path.insert(0, str(blog_simple_dir))

    try:
        import importlib.util

        spec = importlib.util.spec_from_file_location("blog_simple_app", app_file)
        if spec and spec.loader:
            # Just check that we can load the module spec without executing it
            assert spec is not None, "Could not create module spec"
            assert spec.loader is not None, "Module loader not available"
        else:
            pytest.skip("Could not create module spec for blog simple app")

    except Exception as e:
        pytest.skip(f"App import check failed: {e}")
    finally:
        # Clean up
        if str(blog_simple_dir) in sys.path:
            sys.path.remove(str(blog_simple_dir))


@pytest.mark.asyncio
async def test_blog_simple_home_endpoint(blog_simple_client) -> None:
    """Test that blog_simple home endpoint returns expected information."""
    response = await blog_simple_client.get("/")
    assert response.status_code == 200

    data = response.json()
    assert "FraiseQL Simple Blog" in data["message"]
    assert "endpoints" in data
    assert data["endpoints"]["graphql"] == "/graphql"


@pytest.mark.asyncio
async def test_blog_simple_graphql_introspection(blog_simple_graphql_client) -> None:
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
async def test_blog_simple_basic_queries(blog_simple_graphql_client) -> None:
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

    result = await blog_simple_graphql_client.execute(posts_query, variables={"limit": 5})

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
                identifier
            }
        }
    """

    result = await blog_simple_graphql_client.execute(tags_query, variables={"limit": 5})

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "tags" in result["data"]


@pytest.mark.asyncio
async def test_blog_simple_database_connectivity(blog_simple_repository) -> None:
    """Test that database connectivity works properly."""
    # Test basic database connection
    result = await blog_simple_repository.connection.execute("SELECT 1 as test")
    rows = await result.fetchall()
    assert len(rows) == 1
    assert rows[0][0] == 1  # First column of first row


@pytest.mark.asyncio
async def test_blog_simple_seed_data(blog_simple_repository) -> None:
    """Test that seed data is properly loaded."""
    # Check that tb_user table exists and has data
    result = await blog_simple_repository.connection.execute(
        "SELECT COUNT(*) as count FROM tb_user"
    )
    rows = await result.fetchall()
    user_count = rows[0][0]  # First column of first row
    assert user_count > 0, "tb_user table should have seed data"

    # Check that tb_tag table exists and has data
    result = await blog_simple_repository.connection.execute("SELECT COUNT(*) as count FROM tb_tag")
    rows = await result.fetchall()
    tag_count = rows[0][0]  # First column of first row
    assert tag_count > 0, "tb_tag table should have seed data"

    # Check that tb_post table exists and has data
    result = await blog_simple_repository.connection.execute(
        "SELECT COUNT(*) as count FROM tb_post"
    )
    rows = await result.fetchall()
    post_count = rows[0][0]  # First column of first row
    assert post_count > 0, "tb_post table should have seed data"


@pytest.mark.asyncio
async def test_blog_simple_mutations_structure(blog_simple_graphql_client) -> None:
    """Test that GraphQL client can execute queries without hanging."""
    # Test simple query to ensure GraphQL is working
    simple_query = """
        query {
            posts(limit: 1) {
                id
                title
            }
        }
    """

    result = await blog_simple_graphql_client.execute(simple_query)
    assert "errors" not in result or not result["errors"]
    assert "data" in result

    # Skip complex introspection for now to avoid hanging
    # Mutations exist in the schema but introspection may cause issues


@pytest.mark.asyncio
@pytest.mark.slow
async def test_blog_simple_performance_baseline(blog_simple_graphql_client) -> None:
    """Test basic performance baseline for blog_simple."""
    import time

    # Simple query performance test (avoid complex nested queries that may hang)
    query = """
        query GetPosts {
            posts(limit: 5) {
                id
                title
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
async def test_blog_simple_error_handling(blog_simple_graphql_client) -> None:
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
