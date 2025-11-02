# Extracted from: docs/reference/database.md
# Block number: 11
from fraiseql import mutation


@mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    result = await db.execute_raw(
        "INSERT INTO users (data) VALUES ($1) RETURNING *",
        {"name": input.name, "email": input.email},
    )
    return User(**result[0])
