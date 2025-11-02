# Extracted from: docs/core/queries-and-mutations.md
# Block number: 3
from fraiseql import query


@query
async def search_users(info, name_filter: str | None = None, limit: int = 10) -> list[User]:
    repo = info.context["repo"]
    filters = {}
    if name_filter:
        filters["name__icontains"] = name_filter
    # Exclusive Rust pipeline handles camelCase conversion and __typename injection
    return await repo.find_rust("v_user", "users", info, **filters, limit=limit)
