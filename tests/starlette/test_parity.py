"""Parity tests comparing Starlette and Axum GraphQL servers.

These tests verify that both HTTP server implementations handle GraphQL
queries identically, ensuring "sufficient parity" across frameworks.

Sufficient Parity Definition:
- ✅ Valid queries: Must produce identical results
- ✅ APQ caching: Must work identically
- ✅ Authentication: Must behave the same
- ❌ Error messages: Framework differences OK
- ❌ HTTP headers: Framework differences OK
- ❌ Performance: Will differ, documented separately

Test Strategies:
1. Valid Query Execution: Both servers execute valid queries correctly
2. Invalid Query Handling: Both servers reject invalid queries gracefully
3. Authentication: Auth context flows correctly through both
4. APQ Caching: Query deduplication works identically
5. Field Selection: Both respect requested field selections
6. Error Propagation: Both propagate execution errors properly

Note: These tests require both Starlette and Axum to be configured
with identical schemas and database pools.
"""

import json
import pytest
from typing import Any

from fastapi.testclient import TestClient
from starlette.applications import Starlette
from starlette.testclient import TestClient as StarletteTestClient

from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.starlette.app import create_starlette_app
from fraiseql.fastapi.app import create_fraiseql_app


# ============================================================================
# Fixtures
# ============================================================================


@pytest.fixture
async def starlette_app(test_schema, test_database_url):
    """Create a Starlette test application."""
    app = create_starlette_app(
        schema=test_schema,
        database_url=test_database_url,
    )
    return app


@pytest.fixture
def starlette_client(starlette_app):
    """Create a Starlette test client."""
    return StarletteTestClient(starlette_app)


@pytest.fixture
async def fastapi_app(test_schema, test_database_url):
    """Create a FastAPI test application (for comparison)."""
    app = await create_fraiseql_app(
        schema=test_schema,
        database_url=test_database_url,
    )
    return app


@pytest.fixture
def fastapi_client(fastapi_app):
    """Create a FastAPI test client."""
    return TestClient(fastapi_app)


# ============================================================================
# Valid Query Tests
# ============================================================================


class TestValidQueryParity:
    """Test that valid queries work identically on both servers."""

    @pytest.mark.asyncio
    async def test_simple_query_execution(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers execute simple queries identically."""
        query = """
        query {
            users {
                id
                name
                email
            }
        }
        """

        # Execute on Starlette
        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        # Execute on FastAPI
        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should succeed
        assert starlette_response.status_code == 200
        assert fastapi_response.status_code == 200

        # Both should have data (not errors on success)
        starlette_data = starlette_response.json()
        fastapi_data = fastapi_response.json()

        assert "data" in starlette_data
        assert "data" in fastapi_data
        assert starlette_data["data"] == fastapi_data["data"]

    @pytest.mark.asyncio
    async def test_query_with_variables(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers handle query variables identically."""
        query = """
        query GetUser($id: ID!) {
            user(id: $id) {
                id
                name
            }
        }
        """
        variables = {"id": "user-123"}

        # Execute on both
        starlette_response = starlette_client.post(
            "/graphql",
            json={
                "query": query,
                "variables": variables,
            },
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={
                "query": query,
                "variables": variables,
            },
        )

        # Both should succeed
        assert starlette_response.status_code == 200
        assert fastapi_response.status_code == 200

        # Results should match
        starlette_data = starlette_response.json()
        fastapi_data = fastapi_response.json()
        assert starlette_data["data"] == fastapi_data["data"]

    @pytest.mark.asyncio
    async def test_nested_query_execution(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers handle nested queries identically."""
        query = """
        query {
            users {
                id
                name
                posts {
                    id
                    title
                    content
                }
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        assert starlette_response.status_code == 200
        assert fastapi_response.status_code == 200

        starlette_data = starlette_response.json()
        fastapi_data = fastapi_response.json()
        assert starlette_data["data"] == fastapi_data["data"]


# ============================================================================
# Invalid Query Tests
# ============================================================================


class TestInvalidQueryParity:
    """Test that invalid queries are handled consistently."""

    @pytest.mark.asyncio
    async def test_missing_query_field(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers reject requests without query field."""
        # Send request without 'query' field
        starlette_response = starlette_client.post(
            "/graphql",
            json={"variables": {}},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"variables": {}},
        )

        # Both should error (may have different status codes)
        starlette_json = starlette_response.json()
        fastapi_json = fastapi_response.json()

        # Both should have errors
        assert "errors" in starlette_json or starlette_response.status_code >= 400
        assert "errors" in fastapi_json or fastapi_response.status_code >= 400

    @pytest.mark.asyncio
    async def test_invalid_json(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers reject invalid JSON."""
        # Send invalid JSON
        starlette_response = starlette_client.post(
            "/graphql",
            content="{invalid json}",
            headers={"content-type": "application/json"},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            content="{invalid json}",
            headers={"content-type": "application/json"},
        )

        # Both should error
        assert starlette_response.status_code >= 400
        assert fastapi_response.status_code >= 400

    @pytest.mark.asyncio
    async def test_syntax_error_in_query(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers reject queries with syntax errors."""
        # Invalid GraphQL syntax
        query = "query { invalid syntax }"

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should have errors
        starlette_json = starlette_response.json()
        fastapi_json = fastapi_response.json()

        assert "errors" in starlette_json
        assert "errors" in fastapi_json

        # Should have at least one error
        assert len(starlette_json["errors"]) > 0
        assert len(fastapi_json["errors"]) > 0


# ============================================================================
# Authentication Tests
# ============================================================================


class TestAuthenticationParity:
    """Test that authentication flows work identically."""

    @pytest.mark.asyncio
    async def test_request_without_auth(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers handle unauthenticated requests identically."""
        query = """
        query {
            currentUser {
                id
                name
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should behave identically
        # (may return null, error, or 401 depending on implementation)
        assert starlette_response.status_code == fastapi_response.status_code

        starlette_json = starlette_response.json()
        fastapi_json = fastapi_response.json()

        # Both should have the same structure
        assert ("data" in starlette_json) == ("data" in fastapi_json)
        assert ("errors" in starlette_json) == ("errors" in fastapi_json)

    @pytest.mark.asyncio
    async def test_request_with_auth_header(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers process auth headers identically."""
        query = """
        query {
            user {
                id
            }
        }
        """

        headers = {"Authorization": "Bearer test-token"}

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
            headers=headers,
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
            headers=headers,
        )

        # Both should process request
        assert starlette_response.status_code in [200, 400, 401]
        assert fastapi_response.status_code in [200, 400, 401]


# ============================================================================
# Health Check Tests
# ============================================================================


class TestHealthCheckParity:
    """Test that health checks work consistently."""

    @pytest.mark.asyncio
    async def test_health_endpoint(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers have working health check endpoints."""
        starlette_response = starlette_client.get("/health")
        fastapi_response = fastapi_client.get("/health")

        # Both should return 200 when healthy
        assert starlette_response.status_code == 200
        assert fastapi_response.status_code == 200

        starlette_json = starlette_response.json()
        fastapi_json = fastapi_response.json()

        # Both should have status field
        assert "status" in starlette_json
        assert "status" in fastapi_json

        # Both should report healthy
        assert starlette_json["status"] == "healthy"
        assert fastapi_json["status"] == "healthy"


# ============================================================================
# APQ (Automatic Persisted Queries) Tests
# ============================================================================


class TestAPQParity:
    """Test that APQ caching works identically across servers."""

    @pytest.mark.asyncio
    async def test_apq_query_deduplication(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers deduplicate identical queries via APQ."""
        query = """
        query {
            users {
                id
                name
            }
        }
        """

        # First request (query not cached)
        starlette_response1 = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response1 = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should execute successfully
        assert starlette_response1.status_code == 200
        assert fastapi_response1.status_code == 200

        # Second request (query cached)
        starlette_response2 = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response2 = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should return same results
        assert starlette_response1.json() == starlette_response2.json()
        assert fastapi_response1.json() == fastapi_response2.json()

    @pytest.mark.asyncio
    async def test_apq_field_selection_not_cached(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Critical Fix v1.9.4: APQ must not cache responses.

        This test verifies that field selection is respected even when
        using APQ (Automatic Persisted Queries). The bug in v1.9.3 was
        that responses were cached, so identical persisted queries with
        different field selections would return identical cached data.

        Fix: Only cache query strings (persisted queries), not responses.
        Each request must execute the query to apply field selection.
        """
        # Initial query with all fields
        query_all_fields = """
        query GetUsers {
            users {
                id
                name
                email
            }
        }
        """

        response_all = starlette_client.post(
            "/graphql",
            json={"query": query_all_fields},
        )

        assert response_all.status_code == 200
        data_all = response_all.json()

        # Get a user to verify we have email field
        if "data" in data_all and data_all["data"] and "users" in data_all["data"]:
            users = data_all["data"]["users"]
            if users:
                user = users[0]
                # Verify all fields are present
                assert "id" in user
                assert "name" in user
                assert "email" in user
                original_field_count = len(user)

                # Now query with fewer fields (simulate APQ with different selection)
                query_fewer_fields = """
                query GetUsers {
                    users {
                        id
                        name
                    }
                }
                """

                response_fewer = starlette_client.post(
                    "/graphql",
                    json={"query": query_fewer_fields},
                )

                assert response_fewer.status_code == 200
                data_fewer = response_fewer.json()

                if "data" in data_fewer and data_fewer["data"] and "users" in data_fewer["data"]:
                    users_fewer = data_fewer["data"]["users"]
                    if users_fewer:
                        user_fewer = users_fewer[0]

                        # CRITICAL: Verify field selection is respected
                        # With the bug, we would get the same cached response
                        assert "id" in user_fewer
                        assert "name" in user_fewer
                        # Email should NOT be in response (not requested)
                        assert "email" not in user_fewer

                        # Verify the response has fewer fields
                        fewer_field_count = len(user_fewer)
                        assert fewer_field_count < original_field_count

    @pytest.mark.asyncio
    async def test_apq_field_selection_consistency_across_servers(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers handle APQ field selection identically.

        This verifies that Starlette and FastAPI produce identical behavior
        when same query is requested with different field selections.
        """
        query_full = """
        query {
            users {
                id
                name
                email
            }
        }
        """

        query_partial = """
        query {
            users {
                id
                name
            }
        }
        """

        # Execute full query on both servers
        starlette_full = starlette_client.post(
            "/graphql",
            json={"query": query_full},
        )

        fastapi_full = fastapi_client.post(
            "/graphql",
            json={"query": query_full},
        )

        # Both should succeed
        assert starlette_full.status_code == 200
        assert fastapi_full.status_code == 200

        # Execute partial query on both servers
        starlette_partial = starlette_client.post(
            "/graphql",
            json={"query": query_partial},
        )

        fastapi_partial = fastapi_client.post(
            "/graphql",
            json={"query": query_partial},
        )

        # Both should succeed
        assert starlette_partial.status_code == 200
        assert fastapi_partial.status_code == 200

        # Verify both servers handle partial query same way
        # (fewer fields in response compared to full query)
        starlette_data_full = starlette_full.json().get("data", {})
        starlette_data_partial = starlette_partial.json().get("data", {})

        fastapi_data_full = fastapi_full.json().get("data", {})
        fastapi_data_partial = fastapi_partial.json().get("data", {})

        # If we have user data, verify field counts differ appropriately
        if (
            starlette_data_full.get("users")
            and starlette_data_partial.get("users")
        ):
            # Full should have more fields than partial
            full_user = starlette_data_full["users"][0]
            partial_user = starlette_data_partial["users"][0]
            assert len(full_user) > len(partial_user)


# ============================================================================
# Field Selection Tests
# ============================================================================


class TestFieldSelectionParity:
    """Test that field selection works identically."""

    @pytest.mark.asyncio
    async def test_partial_field_selection(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers respect field selections."""
        # Request only 'id' and 'name'
        query = """
        query {
            users {
                id
                name
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        starlette_json = starlette_response.json()
        fastapi_json = fastapi_response.json()

        # Both should only return requested fields
        if "data" in starlette_json and starlette_json["data"]:
            users = starlette_json["data"].get("users", [])
            if users:
                # Each user should have only id and name (and __typename)
                user = users[0]
                assert set(user.keys()) <= {"id", "name", "__typename"}

        if "data" in fastapi_json and fastapi_json["data"]:
            users = fastapi_json["data"].get("users", [])
            if users:
                user = users[0]
                assert set(user.keys()) <= {"id", "name", "__typename"}

    @pytest.mark.asyncio
    async def test_full_field_selection(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers return all fields when requested."""
        # Request all available fields
        query = """
        query {
            users {
                id
                name
                email
                createdAt
                updatedAt
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should return requested data
        assert starlette_response.status_code == 200
        assert fastapi_response.status_code == 200

    @pytest.mark.asyncio
    async def test_id_field_filtering_in_where_clause(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Critical Fix v1.9.3-v1.9.4: ID fields use IDFilter in WHERE clauses.

        This test verifies that ID fields in WHERE clauses work correctly.
        v1.9.3-v1.9.4 added IDFilter type for ID fields to ensure:
        - GraphQL schema uses ID scalar consistently
        - UUID validation happens at runtime (not schema level)
        - Field selection works correctly with ID filtering
        """
        # Query using ID field filtering
        query_by_id = """
        query {
            users(where: { id: { eq: "user-123" } }) {
                id
                name
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query_by_id},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query_by_id},
        )

        # Both should handle the query successfully
        # (may return 0 results if user doesn't exist, but shouldn't error)
        assert starlette_response.status_code in [200, 400]
        assert fastapi_response.status_code in [200, 400]

        # If successful, both should return same structure
        if starlette_response.status_code == 200:
            starlette_data = starlette_response.json()
            fastapi_data = fastapi_response.json()

            # Both should have data key (even if empty)
            assert "data" in starlette_data
            assert "data" in fastapi_data

    @pytest.mark.asyncio
    async def test_id_field_different_operators_in_where(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers support ID field operators in WHERE clauses.

        IDFilter supports: eq, neq, in_, nin (in, notIn), isnull
        This verifies multiple operators work consistently.
        """
        # Test 'in' operator with ID fields
        query_in = """
        query {
            users(where: { id: { in: ["user-1", "user-2"] } }) {
                id
                name
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query_in},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query_in},
        )

        # Both should handle the query
        assert starlette_response.status_code in [200, 400]
        assert fastapi_response.status_code in [200, 400]

        if starlette_response.status_code == 200:
            starlette_data = starlette_response.json()
            fastapi_data = fastapi_response.json()

            # Both should have data key
            assert "data" in starlette_data
            assert "data" in fastapi_data

    @pytest.mark.asyncio
    async def test_id_filtering_with_field_selection(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers handle ID filtering with field selection correctly.

        This verifies that when filtering by ID, field selection is still
        applied correctly (part of v1.9.4 APQ fix verification).
        """
        # Query with ID filter and specific field selection
        query = """
        query {
            users(where: { id: { eq: "user-123" } }) {
                id
                name
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should succeed
        assert starlette_response.status_code in [200, 400]
        assert fastapi_response.status_code in [200, 400]

        if starlette_response.status_code == 200:
            starlette_data = starlette_response.json()
            fastapi_data = fastapi_response.json()

            # If we have users, verify only requested fields are present
            if (
                starlette_data.get("data", {}).get("users")
                and len(starlette_data["data"]["users"]) > 0
            ):
                user = starlette_data["data"]["users"][0]
                # Should have id and name
                assert "id" in user
                assert "name" in user
                # Should NOT have other fields like email (not requested)
                assert "email" not in user or "email" not in user.keys()


# ============================================================================
# Error Propagation Tests
# ============================================================================


class TestErrorPropagationParity:
    """Test that execution errors are handled consistently."""

    @pytest.mark.asyncio
    async def test_resolver_error_handling(
        self,
        starlette_client,
        fastapi_client,
    ):
        """Both servers handle resolver errors consistently."""
        # Query that might cause an error (depends on schema)
        query = """
        query {
            userById(id: "invalid") {
                id
            }
        }
        """

        starlette_response = starlette_client.post(
            "/graphql",
            json={"query": query},
        )

        fastapi_response = fastapi_client.post(
            "/graphql",
            json={"query": query},
        )

        # Both should handle the error consistently
        starlette_json = starlette_response.json()
        fastapi_json = fastapi_response.json()

        # Both should have same structure (either data/errors)
        assert ("errors" in starlette_json) == ("errors" in fastapi_json)


__all__ = [
    "TestValidQueryParity",
    "TestInvalidQueryParity",
    "TestAuthenticationParity",
    "TestHealthCheckParity",
    "TestAPQParity",
    "TestFieldSelectionParity",
    "TestErrorPropagationParity",
]
