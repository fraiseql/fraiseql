"""
Simple Blog Demo Test Configuration
Pytest configuration and fixtures for basic blog functionality testing.
"""

import sys
from pathlib import Path

import pytest
import pytest_asyncio

# Add shared test utilities to path
shared_path = Path(__file__).parent.parent.parent / "shared"
sys.path.insert(0, str(shared_path))

# Import shared database fixtures
from fixtures.database.setup import (
    database_simple,
    db_connection_simple,
    db_manager_simple,
    seeded_blog_database_simple
)

# Import shared GraphQL client
from utilities.graphql_client import (
    simple_graphql_client,
    seeded_blog_data
)


@pytest_asyncio.fixture
async def blog_e2e_workflow(simple_graphql_client, seeded_blog_data):
    """
    Provides utilities and helper functions for E2E workflow testing with real database.

    Returns a dictionary with helper functions for:
    - Creating test users via real GraphQL mutations
    - Setting up blog content via real database operations
    - Simulating user interactions with actual API calls
    """

    async def create_test_user(username_suffix: str = None):
        """Create a test user via real GraphQL mutation."""
        import uuid
        suffix = username_suffix or uuid.uuid4().hex[:8]

        user_input = {
            "username": f"testuser_{suffix}",
            "email": f"testuser_{suffix}@example.com",
            "password": "TestPassword123!",
            "role": "AUTHOR"
        }

        mutation = """
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                __typename
                id
                username
                email
                role
            }
        }
        """

        result = await simple_graphql_client.execute(mutation, {"input": user_input})

        # Handle potential errors in real GraphQL responses
        if "errors" in result:
            raise Exception(f"GraphQL error creating user: {result['errors']}")

        return result["data"]["createUser"]

    async def create_test_post(author_id: str, title_suffix: str = None):
        """Create a test post via real GraphQL mutation."""
        import uuid
        suffix = title_suffix or uuid.uuid4().hex[:8]

        post_input = {
            "title": f"Test Post {suffix}",
            "content": f"This is test content for post {suffix}. It contains multiple paragraphs and demonstrates the blog functionality.",
            "excerpt": f"Test excerpt for post {suffix}",
            "status": "draft",
            "authorId": author_id
        }

        mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                __typename
                id
                title
                slug
                content
                status
                author {
                    id
                    username
                }
            }
        }
        """

        result = await simple_graphql_client.execute(mutation, {"input": post_input})

        # Handle potential errors in real GraphQL responses
        if "errors" in result:
            raise Exception(f"GraphQL error creating post: {result['errors']}")

        return result["data"]["createPost"]

    async def simulate_user_journey(steps: list[str]):
        """Simulate a complete user journey through the real blog application."""
        journey_results = {}

        if "register" in steps:
            user = await create_test_user()
            journey_results["user"] = user

        if "create_post" in steps and "user" in journey_results:
            post = await create_test_post(journey_results["user"]["id"])
            journey_results["post"] = post

        return journey_results

    # Return workflow utilities
    return {
        "create_test_user": create_test_user,
        "create_test_post": create_test_post,
        "simulate_user_journey": simulate_user_journey,
        "seeded_data": seeded_blog_data,
    }


# Pytest configuration
def pytest_configure(config):
    """Configure pytest for async testing and custom markers."""
    # Add custom markers
    config.addinivalue_line("markers", "e2e: marks tests as end-to-end tests")
    config.addinivalue_line("markers", "blog_demo: marks tests as blog demo specific")
    config.addinivalue_line("markers", "slow: marks tests as slow running")
    config.addinivalue_line("markers", "performance: marks tests as performance tests")
    config.addinivalue_line("markers", "security: marks tests as security tests")
    config.addinivalue_line("markers", "database: marks tests requiring database")
