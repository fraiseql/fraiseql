# Extracted from: docs/core/queries-and-mutations.md
# Block number: 20
from fraiseql import mutation


@mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    user_data = {"name": input.name, "email": input.email, "created_at": datetime.utcnow()}
    result = await db.execute_raw("INSERT INTO users (data) VALUES ($1) RETURNING *", user_data)
    return User(**result[0]["data"])
