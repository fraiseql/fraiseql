# Extracted from: docs/reference/database.md
# Block number: 32
from fraiseql import query


@query
async def get_user(info, id: UUID) -> User | None:
    try:
        db = info.context["db"]
        user = await db.find_one("v_user", where={"id": id})
        if not user:
            return None
        return User(**user)
    except Exception as e:
        logger.error(f"Failed to fetch user {id}: {e}")
        raise GraphQLError("Failed to fetch user")
