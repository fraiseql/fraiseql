# Extracted from: docs/reference/decorators.md
# Block number: 22
from fraiseql import mutation, query
from fraiseql.auth import requires_auth


@query
@requires_auth
async def get_my_profile(info) -> User:
    user = info.context["user"]  # Guaranteed to be authenticated
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": user.user_id})


@mutation
@requires_auth
async def update_profile(info, input: UpdateProfileInput) -> User:
    user = info.context["user"]
    db = info.context["db"]
    return await db.update_one("v_user", where={"id": user.user_id}, updates=input.__dict__)
