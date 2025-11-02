# Extracted from: docs/reference/decorators.md
# Block number: 26
from fraiseql import mutation, query
from fraiseql.auth import requires_role


@query
@requires_role("admin")
async def get_all_users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")


@mutation
@requires_role("admin")
async def admin_action(info, input: AdminActionInput) -> Result:
    # Admin-only mutation
    pass
