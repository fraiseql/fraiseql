# Extracted from: docs/strategic/V1_VISION.md
# Block number: 1
from fraiseql import mutation


@mutation
async def create_user(info, organisation: str, identifier: str, name: str, email: str) -> User:
    db = info.context["db"]
    id = await db.fetchval(
        "SELECT fn_create_user($1, $2, $3, $4)", organisation, identifier, name, email
    )
    return await QueryRepository(db).find_one("tv_user", id=id)
