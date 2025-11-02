# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 14
async def test_user_creation(repo):
    from tests.unit.utils.test_response_utils import extract_graphql_data

    result = await repo.find("users")
    users = extract_graphql_data(result, "users")

    assert len(users) == 1
    assert users[0]["firstName"] == "Test User"
