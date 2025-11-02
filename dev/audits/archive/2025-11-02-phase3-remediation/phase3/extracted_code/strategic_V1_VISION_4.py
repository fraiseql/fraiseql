# Extracted from: docs/strategic/V1_VISION.md
# Block number: 4
from fraiseql import mutation, query


@query
async def user(info, id: UUID = None, identifier: str = None) -> User:
    """Get user by UUID or identifier"""
    repo = QueryRepository(info.context["db"])
    if id:
        return await repo.find_one("tv_user", id=id)
    if identifier:
        return await repo.find_one("tv_user", identifier=identifier)


@mutation
async def create_user(info, organisation: str, identifier: str, name: str, email: str) -> User:
    """Create user (business logic in database function)"""
    db = info.context["db"]
    id = await db.fetchval(
        "SELECT fn_create_user($1, $2, $3, $4)", organisation, identifier, name, email
    )
    return await QueryRepository(db).find_one("tv_user", id=id)
