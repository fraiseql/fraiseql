# Extracted from: docs/strategic/V1_ADVANCED_PATTERNS.md
# Block number: 3
from fraiseql import mutation


@mutation
async def create_user(info, name: str, email: str) -> User:
    db = info.context["db"]

    # ❌ Business logic in Python (not reusable)
    if not email_is_valid(email):
        raise ValueError("Invalid email")

    # ❌ Manual transaction management
    async with db.transaction():
        id = await db.fetchval(
            "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id", name, email
        )

        # ❌ Manual sync (can forget!)
        await sync_tv_user(db, id)

    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)
