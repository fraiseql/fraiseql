# Extracted from: docs/core/project-structure.md
# Block number: 2
# src/queries/user_queries.py
from fraiseql import fraise_field, type

from ..types.user import User


@type
class UserQueries:
    """User-related query operations."""

    users: list[User] = fraise_field(description="List all users")
    user_by_username: User | None = fraise_field(description="Find user by username")

    async def resolve_users(self, info):
        repo = info.context["repo"]
        results = await repo.find("v_user")
        return [User(**result) for result in results]

    async def resolve_user_by_username(self, info, username: str):
        repo = info.context["repo"]
        result = await repo.find_one("v_user", where={"username": username})
        return User(**result) if result else None
