# Extracted from: docs/advanced/authentication.md
# Block number: 23
from fraiseql import type_
from fraiseql.security import any_permission, authorize_field


@type_
class User:
    id: UUID
    name: str
    email: str

    # Only admins or user themselves can see email
    @authorize_field(
        lambda user, info: (
            info.context["user"].user_id == user.id or info.context["user"].has_role("admin")
        )
    )
    async def email(self) -> str:
        return self._email

    # Only admins can see internal notes
    @authorize_field(any_permission("admin:all"))
    async def internal_notes(self) -> str | None:
        return self._internal_notes
