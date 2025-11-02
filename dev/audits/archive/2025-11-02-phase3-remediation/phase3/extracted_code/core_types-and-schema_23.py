# Extracted from: docs/core/types-and-schema.md
# Block number: 23
from fraiseql import mutation


@mutation
async def update_user(info, input: UpdateUserInput) -> User:
    db = info.context["db"]
    updates = {}

    # Only include fields that were explicitly provided
    if input.name is not UNSET:
        updates["name"] = input.name  # Could be None (clear) or str (update)
    if input.email is not UNSET:
        updates["email"] = input.email
    if input.bio is not UNSET:
        updates["bio"] = input.bio

    return await db.update_one("v_user", {"id": input.id}, updates)
