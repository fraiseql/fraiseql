# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 16
# Test Rust pipeline responses
result = await repo.find_rust("v_user", "users", info)
assert isinstance(result, RustResponseBytes)
assert result.bytes.startswith(b'{"data"')

# Test GraphQL integration
response = client.post("/graphql", json={"query": "{ users { id } }"})
assert response.json()["data"]["users"]  # Works seamlessly
