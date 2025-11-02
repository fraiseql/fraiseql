# Extracted from: docs/core/concepts-glossary.md
# Block number: 7


async def get_users(info) -> list[User]:
    """Get all users."""
    db = info.context["db"]
    return await db.find(User)
