# Extracted from: docs/tutorials/INTERACTIVE_EXAMPLES.md
# Block number: 2
from fraiseql import query


@query
async def users(self, info, email_filter: str | None = None) -> list[User]:
    filters = {}
    if email_filter:
        filters["email__icontains"] = email_filter

    return await repo.find_rust("v_user", "users", info, **filters)
