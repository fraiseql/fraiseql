# Extracted from: docs/production/security.md
# Block number: 1

# SAFE: Parameterized query
async def get_user(user_id: str) -> User:
    async with db.connection() as conn:
        result = await conn.execute(
            "SELECT * FROM users WHERE id = $1",
            user_id,  # Automatically escaped
        )
        return result.fetchone()


# UNSAFE: String interpolation (never do this!)
# async def get_user_unsafe(user_id: str) -> User:
#     query = f"SELECT * FROM users WHERE id = '{user_id}'"
#     result = await conn.execute(query)  # VULNERABLE
