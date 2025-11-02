# Extracted from: docs/core/rust-pipeline-integration.md
# Block number: 1
from fraiseql import query, type


# 1. Define GraphQL type
@type(sql_source="v_user")
class User:
    id: UUID
    first_name: str  # Python uses snake_case
    created_at: datetime


# 2. Define query resolver
@query
async def users(info) -> list[User]:
    repo = info.context["repo"]

    # 3. Execute PostgreSQL query (returns JSONB)
    # Rust pipeline handles transformation automatically
    return await repo.find("v_user")
