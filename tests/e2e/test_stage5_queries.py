"""
Stage 5: Execute GraphQL queries via HTTP against a running FraiseQL server.

Run with:
    FRAISEQL_TEST_URL=http://localhost:17843 pytest tests/e2e/test_stage5_queries.py -v

Requires:
    - A running FraiseQL server (started by `make e2e-setup`)
    - The `FRAISEQL_TEST_URL` environment variable set to the server URL
"""

import os

import pytest
import requests

pytestmark = pytest.mark.skipif(
    not os.getenv("FRAISEQL_TEST_URL"),
    reason="Set FRAISEQL_TEST_URL to run E2E tests",
)

BASE_URL = os.getenv("FRAISEQL_TEST_URL", "http://localhost:17843")
GRAPHQL_ENDPOINT = f"{BASE_URL}/graphql"


def gql(query: str, variables: dict | None = None) -> dict:
    """Execute a GraphQL request and return the parsed response."""
    payload: dict = {"query": query}
    if variables:
        payload["variables"] = variables
    response = requests.post(GRAPHQL_ENDPOINT, json=payload, timeout=10)
    response.raise_for_status()
    data = response.json()
    assert "errors" not in data, f"GraphQL errors: {data['errors']}"
    return data["data"]


def test_authors_query_returns_list() -> None:
    """authors query must return a JSON array."""
    result = gql("{ authors { pkAuthorId name email } }")
    assert "authors" in result
    assert isinstance(result["authors"], list)


def test_posts_query_returns_list() -> None:
    """posts query must return a JSON array."""
    result = gql("{ posts { pkPostId title published } }")
    assert "posts" in result
    assert isinstance(result["posts"], list)


def test_create_author_mutation() -> None:
    """createAuthor mutation must return a valid pkAuthorId."""
    result = gql(
        "mutation($name: String!, $email: String!) { createAuthor(name: $name, email: $email) { pkAuthorId name email } }",
        variables={"name": "E2E Test Author", "email": "e2e@fraiseql.test"},
    )
    assert "createAuthor" in result
    author = result["createAuthor"]
    assert isinstance(author["pkAuthorId"], int)
    assert author["name"] == "E2E Test Author"
    assert author["email"] == "e2e@fraiseql.test"


def test_create_post_mutation() -> None:
    """createPost mutation must return a valid pkPostId and title."""
    # First create an author to own the post
    author_result = gql(
        "mutation($name: String!, $email: String!) { createAuthor(name: $name, email: $email) { pkAuthorId } }",
        variables={"name": "Post Author E2E", "email": "postauthor@fraiseql.test"},
    )
    author_id = author_result["createAuthor"]["pkAuthorId"]

    post_result = gql(
        "mutation($title: String!, $body: String!, $fkAuthorId: Int!) { createPost(title: $title, body: $body, fkAuthorId: $fkAuthorId) { pkPostId title } }",
        variables={
            "title": "E2E Test Post",
            "body": "This post was created by the E2E pipeline test.",
            "fkAuthorId": author_id,
        },
    )
    assert "createPost" in post_result
    post = post_result["createPost"]
    assert isinstance(post["pkPostId"], int)
    assert post["title"] == "E2E Test Post"


def test_introspection_responds() -> None:
    """GraphQL introspection must return __schema."""
    result = gql("{ __schema { queryType { name } } }")
    assert "__schema" in result
    assert result["__schema"]["queryType"]["name"] == "Query"
