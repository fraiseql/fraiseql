# Extracted from: docs/api-reference/database.md
# Block number: 3
users = await repo.find(
    "users_view", limit=10, where={"is_active": True}, order_by={"created_at": "desc"}
)
