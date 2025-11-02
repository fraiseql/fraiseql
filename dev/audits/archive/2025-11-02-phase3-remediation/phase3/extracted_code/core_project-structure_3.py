# Extracted from: docs/core/project-structure.md
# Block number: 3
# src/mutations/user_mutations.py
from fraiseql import fraise_field, input, type

from ..types.user import User


@input
class CreateUserInput:
    """Input for creating a new user."""

    username: str = fraise_field(description="Desired username")
    email: str = fraise_field(description="Email address")


@type
class UserMutations:
    """User-related mutation operations."""

    create_user: User = fraise_field(description="Create a new user account")

    async def resolve_create_user(self, info, input: CreateUserInput):
        repo = info.context["repo"]
        user_id = await repo.call_function(
            "fn_create_user", p_username=input.username, p_email=input.email
        )
        result = await repo.find_one("v_user", where={"id": user_id})
        return User(**result)
