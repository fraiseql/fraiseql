# Extracted from: docs/api-reference/database.md
# Block number: 13
repo_with_context = FraiseQLRepository(
    pool, context={"user_id": current_user_id, "tenant_id": tenant_id}
)

# Context is available in queries
users = await repo_with_context.find("users_view")
