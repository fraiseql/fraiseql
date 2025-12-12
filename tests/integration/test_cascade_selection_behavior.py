"""Test CASCADE selection behavior - verify data only returned when requested."""

import json

import pytest

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_cascade_data_not_included_when_not_requested(cascade_http_client):
    """CRITICAL TEST: Verify CASCADE data is NOT included when not in GraphQL selection.

    This tests that PostgreSQL may return CASCADE data, but Rust should filter it
    out if the GraphQL query doesn't request the cascade field.
    """
    # Query WITHOUT cascade field in selection
    mutation_query = """
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
    """

    variables = {
        "input": {
            "title": "Test Without CASCADE",
            "content": "Testing selection behavior",
            "authorId": "user-123",
        }
    }

    response = await cascade_http_client.post(
        "/graphql", json={"query": mutation_query, "variables": variables}
    )

    assert response.status_code == 200
    data = response.json()

    print("\n=== RESPONSE WITHOUT CASCADE IN SELECTION ===")
    print(json.dumps(data, indent=2))
    print("=" * 60)

    result = data["data"]["createPostWithEntity"]

    # Verify entity fields are present
    assert "message" in result
    assert "post" in result
    assert result["post"]["title"] == "Test Without CASCADE"

    # Check CASCADE field presence
    if "cascade" in result:
        cascade_value = result["cascade"]
        print("\n⚠️  CASCADE field found in response!")
        print(f"CASCADE value: {cascade_value}")
        print(f"CASCADE type: {type(cascade_value)}")

        if cascade_value is None:
            print("✓ CASCADE is None (field exists in schema but empty)")
            # This is acceptable - field defined in schema but no data
        else:
            print("❌ FAIL: CASCADE contains data even though not requested!")
            print(f"   Updated: {len(cascade_value.get('updated', []))} items")
            print("   This is a bug - CASCADE data should not be returned")
            pytest.fail(
                "CASCADE data present in response even though not requested in selection. "
                "PostgreSQL returned CASCADE data but Rust should filter it based on GraphQL selection."
            )
    else:
        print("\n✓ PERFECT: CASCADE field not in response (ideal behavior)")


@pytest.mark.asyncio
async def test_cascade_data_included_when_requested(cascade_http_client):
    """Verify CASCADE data IS included when requested in GraphQL selection."""
    # Query WITH cascade field in selection
    mutation_query = """
    mutation CreatePostWithEntity($input: CreatePostInput!) {
        createPostWithEntity(input: $input) {
            ... on CreatePostWithEntitySuccess {
                message
                post {
                    id
                    title
                }
                cascade {
                    updated {
                        id
                        operation
                    }
                    deleted {
                        id
                        operation
                    }
                    metadata {
                        affectedCount
                    }
                }
            }
        }
    }
    """

    variables = {
        "input": {
            "title": "Test With CASCADE",
            "content": "Testing CASCADE requested",
            "authorId": "user-123",
        }
    }

    response = await cascade_http_client.post(
        "/graphql", json={"query": mutation_query, "variables": variables}
    )

    assert response.status_code == 200
    data = response.json()

    print("\n=== RESPONSE WITH CASCADE IN SELECTION ===")
    print(json.dumps(data, indent=2))
    print("=" * 60)

    result = data["data"]["createPostWithEntity"]

    # CASCADE must be present and populated
    assert "cascade" in result, "CASCADE must be in response when requested"
    assert result["cascade"] is not None, "CASCADE must not be None when requested"

    cascade = result["cascade"]
    assert "updated" in cascade
    assert "deleted" in cascade
    assert "metadata" in cascade
    assert cascade["metadata"]["affectedCount"] > 0

    print(f"\n✓ CASCADE correctly returned: {cascade['metadata']['affectedCount']} affected")
