# Extracted from: docs/migration/v0-to-v1.md
# Block number: 10
users = await repo.find(
    "users_view", where={"age": {"gte": 18, "lt": 65}, "status": {"in": ["active", "pending"]}}
)
