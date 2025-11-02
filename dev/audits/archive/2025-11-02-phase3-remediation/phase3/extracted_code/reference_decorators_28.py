# Extracted from: docs/reference/decorators.md
# Block number: 28
from fraiseql import mutation
from fraiseql.auth import requires_any_permission


@mutation
@requires_any_permission("users:write", "admin:all")
async def update_user(info, id: UUID, input: UpdateUserInput) -> User:
    # Can be performed by users:write OR admin:all
    db = info.context["db"]
    return await db.update_one("v_user", where={"id": id}, updates=input.__dict__)
