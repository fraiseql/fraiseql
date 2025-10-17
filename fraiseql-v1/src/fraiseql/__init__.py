"""
FraiseQL v1 - High-Performance GraphQL Framework

A Python GraphQL framework built for sub-1ms queries using CQRS architecture
and Rust transformation.

Key Features:
- CQRS with explicit command/query separation
- Rust-powered JSON transformation (40x speedup)
- PostgreSQL JSONB-optimized queries
- Clean decorator API
- Production-grade patterns

Example:
    from fraiseql import FraiseQL, type, query, mutation
    from fraiseql.repositories import QueryRepository, CommandRepository

    @type
    class User:
        id: UUID
        name: str
        email: str

    @query
    async def user(info, id: UUID) -> User:
        repo = QueryRepository(info.context["db"])
        return await repo.find_one("tv_user", id=id)

    @mutation
    async def create_user(info, name: str, email: str) -> User:
        db = info.context["db"]
        cmd_repo = CommandRepository(db)

        user_id = await cmd_repo.execute(
            "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id",
            name, email
        )

        await sync_tv_user(db, user_id)

        query_repo = QueryRepository(db)
        return await query_repo.find_one("tv_user", id=user_id)
"""

__version__ = "1.0.0-alpha"

# Public API exports will be added as components are implemented:
# from fraiseql.types import type, input, field
# from fraiseql.decorators import query, mutation, subscription
# from fraiseql.repositories import CommandRepository, QueryRepository
# from fraiseql.repositories.sync import sync_tv_*
# from fraiseql.core import FraiseQL

__all__ = [
    "__version__",
    # Add exports as components are built
]
