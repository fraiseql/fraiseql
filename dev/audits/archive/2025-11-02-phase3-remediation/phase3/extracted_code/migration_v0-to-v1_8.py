# Extracted from: docs/migration/v0-to-v1.md
# Block number: 8
from fraiseql import Info, query


@query
def get_users(info: Info, limit: int = 10) -> list[User]:
    return info.context.repo.find("users_view", limit=limit)
