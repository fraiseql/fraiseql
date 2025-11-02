# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 1
from uuid import UUID

from fraiseql import mutation, query, type


@type
class Organisation:
    id: UUID  # ✅ Clean! Just "id" (UUID)
    identifier: str  # "acme-corp"
    name: str


@type
class User:
    id: UUID  # ✅ Clean! Just "id" (UUID)
    identifier: str  # "john-doe"
    name: str
    email: str
    organisation: Organisation


@type
class Post:
    id: UUID  # ✅ Clean! Just "id" (UUID)
    identifier: str  # "my-first-post"
    title: str
    content: str
    author: User


# Query by UUID or identifier
@query
async def user(info, id: UUID | None = None, identifier: str | None = None) -> User | None:
    """Get user by UUID or identifier"""
    repo = QueryRepository(info.context["db"])

    if id:
        return await repo.find_one("tv_user", id=id)
    if identifier:
        return await repo.find_by_identifier("tv_user", identifier)
    raise ValueError("Must provide id or identifier")


# Mutations return UUID
@mutation
async def create_user(
    info,
    organisation: str,  # Organisation identifier (human-friendly!)
    identifier: str,  # User identifier (username)
    name: str,
    email: str,
) -> User:
    """Create user with human-friendly identifiers"""
    db = info.context["db"]

    # Function returns UUID
    id = await db.fetchval(
        "SELECT fn_create_user($1, $2, $3, $4)", organisation, identifier, name, email
    )

    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)
