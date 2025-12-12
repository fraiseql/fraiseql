"""Performance tests for CASCADE selection filtering."""

import json

import pytest

pytestmark = pytest.mark.integration


class TestCascadePerformance:
    """Verify CASCADE filtering improves performance."""

    @pytest.mark.asyncio
    async def test_response_size_reduction(self, cascade_http_client):
        """Verify response size is smaller without CASCADE."""
        # Without CASCADE
        response_without = await cascade_http_client.post(
            "/graphql",
            json={
                "query": """
                mutation CreatePostWithEntity($input: CreatePostInput!) {
                    createPostWithEntity(input: $input) {
                        ... on CreatePostWithEntitySuccess {
                            message
                            post {
                                id
                                title
                                content
                            }
                        }
                    }
                }
                """,
                "variables": {
                    "input": {"title": "Test", "content": "Test", "authorId": "user-123"}
                },
            },
        )

        # With CASCADE
        response_with = await cascade_http_client.post(
            "/graphql",
            json={
                "query": """
                mutation CreatePostWithEntity($input: CreatePostInput!) {
                    createPostWithEntity(input: $input) {
                        ... on CreatePostWithEntitySuccess {
                            message
                            post {
                                id
                                title
                                content
                            }
                            cascade {
                                updated { __typename id operation entity }
                                deleted { __typename id }
                                invalidations { queryName strategy scope }
                                metadata { timestamp affectedCount }
                            }
                        }
                    }
                }
                """,
                "variables": {
                    "input": {"title": "Test2", "content": "Test2", "authorId": "user-123"}
                },
            },
        )

        assert response_without.status_code == 200
        assert response_with.status_code == 200

        result_without = response_without.json()
        result_with = response_with.json()

        # Measure sizes
        size_without = len(json.dumps(result_without).encode("utf-8"))
        size_with = len(json.dumps(result_with).encode("utf-8"))

        # Without CASCADE should be significantly smaller
        reduction_ratio = size_with / size_without

        assert reduction_ratio > 1.5, (
            f"CASCADE should add significant data. Ratio: {reduction_ratio:.2f}x (expected > 1.5x)"
        )

        print("\nPayload size reduction:")
        print(f"  Without CASCADE: {size_without} bytes")
        print(f"  With CASCADE: {size_with} bytes")
        print(f"  Ratio: {reduction_ratio:.2f}x")
