# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 13
async def test_user_creation(repo):
    result = await repo.find("users")
    assert len(result) == 1
    assert result[0].first_name == "Test User"
