"""GraphQL specification compliance tests for CASCADE."""

import pytest

pytestmark = pytest.mark.integration


class TestCascadeGraphQLSpec:
    """Verify CASCADE follows GraphQL specification."""

    @pytest.mark.asyncio
    async def test_cascade_only_returned_when_selected(self, cascade_http_client):
        """GraphQL spec: Only return fields that are selected."""
        # Test 1: Field not selected
        response1 = await cascade_http_client.post(
            "/graphql",
            json={
                "query": """
                mutation CreatePostWithEntity($input: CreatePostInput!) {
                    createPostWithEntity(input: $input) {
                        ... on CreatePostWithEntitySuccess {
                            message
                        }
                    }
                }
                """,
                "variables": {
                    "input": {"title": "Test", "content": "Test", "authorId": "user-123"}
                },
            },
        )

        assert response1.status_code == 200
        result1 = response1.json()

        assert "cascade" not in result1["data"]["createPostWithEntity"]

        # Test 2: Field selected
        response2 = await cascade_http_client.post(
            "/graphql",
            json={
                "query": """
                mutation CreatePostWithEntity($input: CreatePostInput!) {
                    createPostWithEntity(input: $input) {
                        ... on CreatePostWithEntitySuccess {
                            message
                            cascade { updated { id } }
                        }
                    }
                }
                """,
                "variables": {
                    "input": {"title": "Test", "content": "Test", "authorId": "user-123"}
                },
            },
        )

        assert response2.status_code == 200
        result2 = response2.json()

        assert "cascade" in result2["data"]["createPostWithEntity"]

    @pytest.mark.asyncio
    async def test_cascade_introspection(self, cascade_http_client):
        """CASCADE field should be visible in introspection."""
        introspection_query = """
            query {
                __type(name: "CreatePostWithEntitySuccess") {
                    name
                    fields {
                        name
                        type {
                            name
                            kind
                        }
                    }
                }
            }
        """

        response = await cascade_http_client.post("/graphql", json={"query": introspection_query})

        assert response.status_code == 200
        result = response.json()

        assert "errors" not in result
        type_info = result["data"]["__type"]

        # Find cascade field
        cascade_field = next((f for f in type_info["fields"] if f["name"] == "cascade"), None)

        assert cascade_field is not None, "CASCADE field should be in schema"
        assert cascade_field["type"]["name"] == "Cascade"
