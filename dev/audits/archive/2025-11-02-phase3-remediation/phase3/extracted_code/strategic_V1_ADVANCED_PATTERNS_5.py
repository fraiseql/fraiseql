# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 5
from fraiseql import mutation


@mutation
async def create_post(
    info,
    author: str,  # Author identifier (username)
    identifier: str,  # Post slug
    title: str,
    content: str,
) -> Post:
    db = info.context["db"]
    id = await db.fetchval(
        "SELECT fn_create_post($1, $2, $3, $4)", author, identifier, title, content
    )
    return await QueryRepository(db).find_one("tv_post", id=id)


@mutation
async def update_post(info, id: UUID, title: str, content: str) -> Post:
    db = info.context["db"]
    id = await db.fetchval("SELECT fn_update_post($1, $2, $3)", id, title, content)
    return await QueryRepository(db).find_one("tv_post", id=id)


@mutation
async def delete_post(info, id: UUID) -> bool:
    db = info.context["db"]
    return await db.fetchval("SELECT fn_delete_post($1)", id)
