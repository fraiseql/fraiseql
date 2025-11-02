# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 7
from uuid import UUID

from fraiseql import mutation, query, type


@type
class Organisation:
    id: UUID
    identifier: str
    name: str


@type
class User:
    id: UUID
    identifier: str
    name: str
    email: str
    organisation: Organisation


@type
class Post:
    id: UUID
    identifier: str
    title: str
    content: str
    author: User


# QUERIES
@query
async def user(info, id: UUID | None = None, identifier: str | None = None) -> User | None:
    repo = QueryRepository(info.context["db"])
    if id:
        return await repo.find_one("tv_user", id=id)
    if identifier:
        return await repo.find_by_identifier("tv_user", identifier)
    raise ValueError("Must provide id or identifier")


# MUTATIONS (trivial - logic in database)
@mutation
async def create_user(info, organisation: str, identifier: str, name: str, email: str) -> User:
    db = info.context["db"]
    id = await db.fetchval(
        "SELECT fn_create_user($1, $2, $3, $4)", organisation, identifier, name, email
    )
    return await QueryRepository(db).find_one("tv_user", id=id)


@mutation
async def create_post(info, author: str, identifier: str, title: str, content: str) -> Post:
    db = info.context["db"]
    id = await db.fetchval(
        "SELECT fn_create_post($1, $2, $3, $4)", author, identifier, title, content
    )
    return await QueryRepository(db).find_one("tv_post", id=id)
