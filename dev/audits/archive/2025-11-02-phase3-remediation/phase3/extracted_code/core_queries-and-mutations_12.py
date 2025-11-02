# Extracted from: docs/core/queries-and-mutations.md
# Block number: 12
from fraiseql import field, type


@type
class User:
    id: UUID

    @field(description="Private user settings (owner only)")
    async def settings(self, info) -> UserSettings | None:
        user_context = info.context.get("user")
        if not user_context or user_context.user_id != self.id:
            return None  # Don't expose private data

        repo = info.context["repo"]
        return await repo.find_one_rust("v_user_settings", "settings", info, user_id=self.id)
