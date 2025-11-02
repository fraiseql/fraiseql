# Extracted from: docs/core/queries-and-mutations.md
# Block number: 24
from fraiseql import input, mutation


@input
class UpdateUserInput:
    id: UUID
    name: str | None = None
    email: str | None = None


@mutation
async def update_user(info, input: UpdateUserInput) -> User:
    db = info.context["db"]
    user_context = info.context.get("user")

    # Authorization check
    if not user_context:
        raise GraphQLError("Authentication required")

    # Validation
    if input.email and not is_valid_email(input.email):
        raise GraphQLError("Invalid email format")

    # Update logic
    updates = {}
    if input.name:
        updates["name"] = input.name
    if input.email:
        updates["email"] = input.email

    if not updates:
        raise GraphQLError("No fields to update")

    return await db.update_one("v_user", where={"id": input.id}, updates=updates)
