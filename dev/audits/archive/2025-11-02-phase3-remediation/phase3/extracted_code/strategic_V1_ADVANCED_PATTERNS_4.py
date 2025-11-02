# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 4
from fraiseql import mutation


@mutation
async def create_user(
    info,
    organisation: str,  # Organisation identifier
    identifier: str,  # Username
    name: str,
    email: str,
) -> User:
    """Create user (business logic in database)"""
    db = info.context["db"]

    # ✅ Just call the function - that's it!
    try:
        id = await db.fetchval(
            "SELECT fn_create_user($1, $2, $3, $4)", organisation, identifier, name, email
        )
    except Exception as e:
        # Database raises meaningful errors
        raise GraphQLError(str(e))

    # Read from query side
    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)
