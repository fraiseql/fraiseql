# Extracted from: docs/reference/database.md
# Block number: 13
from fraiseql import mutation


@mutation
async def update_user(info, id: UUID, input: UpdateUserInput) -> User:
    db = info.context["db"]
    result = await db.execute_raw(
        """
        UPDATE users
        SET data = data || $1::jsonb
        WHERE id = $2
        RETURNING *
        """,
        input.__dict__,
        id,
    )
    return User(**result[0])
