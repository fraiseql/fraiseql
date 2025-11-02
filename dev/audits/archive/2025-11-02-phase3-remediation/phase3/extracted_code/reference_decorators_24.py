# Extracted from: docs/reference/decorators.md
# Block number: 24
from fraiseql import mutation
from fraiseql.auth import requires_permission


@mutation
@requires_permission("users:write")
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    return await db.create_one("v_user", data=input.__dict__)


@mutation
@requires_permission("users:delete")
async def delete_user(info, id: UUID) -> bool:
    db = info.context["db"]
    await db.delete_one("v_user", where={"id": id})
    return True
