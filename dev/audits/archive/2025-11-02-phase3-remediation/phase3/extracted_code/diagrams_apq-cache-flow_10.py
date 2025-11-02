# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 10
def test_apq_flow():
    # Test cache miss
    response = client.post(
        "/graphql",
        json={
            "query": "query { test }",
            "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "unknown"}},
        },
    )
    assert response.status_code == 200
    assert "PersistedQueryNotFound" in response.json()["errors"][0]["message"]

    # Test cache population
    response = client.post(
        "/graphql",
        json={
            "query": "query { test }",
            "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}},
        },
    )
    assert response.status_code == 200
    # Query should now be cached

    # Test cache hit
    response = client.post(
        "/graphql",
        json={
            "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}},
            "variables": {},
        },
    )
    assert response.status_code == 200
    # Should use cached query
