# Extracted from: docs/advanced/authentication.md
# Block number: 10
from fraiseql.auth.native import NativeAuthFactory, UserRepository


# 1. Implement user repository
class PostgresUserRepository(UserRepository):
    """User repository backed by PostgreSQL."""

    async def get_user_by_username(self, username: str) -> User | None:
        async with db.connection() as conn:
            result = await conn.execute("SELECT * FROM users WHERE username = $1", username)
            row = await result.fetchone()
            return User(**row) if row else None

    async def get_user_by_id(self, user_id: str) -> User | None:
        async with db.connection() as conn:
            result = await conn.execute("SELECT * FROM users WHERE id = $1", user_id)
            row = await result.fetchone()
            return User(**row) if row else None

    async def create_user(self, username: str, password_hash: str, email: str) -> User:
        async with db.connection() as conn:
            result = await conn.execute(
                "INSERT INTO users (username, password_hash, email) VALUES ($1, $2, $3) RETURNING *",
                username,
                password_hash,
                email,
            )
            row = await result.fetchone()
            return User(**row)


# 2. Create provider
user_repo = PostgresUserRepository()

auth_provider = NativeAuthFactory.create_provider(
    user_repository=user_repo,
    secret_key="your-secret-key",
    access_token_ttl=3600,  # 1 hour
    refresh_token_ttl=2592000,  # 30 days
)

# 3. Mount authentication routes
from fraiseql.auth.native import create_auth_router

auth_router = create_auth_router(auth_provider)
app.include_router(auth_router, prefix="/auth")
