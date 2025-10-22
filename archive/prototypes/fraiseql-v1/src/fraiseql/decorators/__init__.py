"""
FraiseQL Decorators

GraphQL resolver decorators for queries, mutations, and subscriptions.

Example:
    from fraiseql.decorators import query, mutation

    @query
    async def user(info, id: UUID) -> User:
        '''Get user by ID'''
        repo = QueryRepository(info.context["db"])
        return await repo.find_one("tv_user", id=id)

    @mutation
    async def create_user(info, name: str) -> User:
        '''Create a new user'''
        db = info.context["db"]
        user_id = await db.fetchval(
            "INSERT INTO tb_user (name) VALUES ($1) RETURNING id",
            name
        )
        await sync_tv_user(db, user_id)
        return await QueryRepository(db).find_one("tv_user", id=user_id)
"""

# Exports will be added as implementation progresses:
# from fraiseql.decorators.query import query
# from fraiseql.decorators.mutation import mutation
# from fraiseql.decorators.subscription import subscription

__all__ = [
    # Add exports as they are implemented
]
