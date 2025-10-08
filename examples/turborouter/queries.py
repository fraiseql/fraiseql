"""GraphQL Query Resolvers."""

from typing import Optional
from schema import User, Post


async def user(info, id: int) -> Optional[User]:
    """Get a single user by ID.

    This query is registered with TurboRouter for fast execution.
    """
    db = info.context["db"]
    return await db.find_one("v_users", id=id)


async def users(info, limit: int = 10, offset: int = 0) -> list[User]:
    """Get a list of users.

    This query is registered with TurboRouter.
    """
    db = info.context["db"]
    return await db.find("v_users", limit=limit, offset=offset)


async def post(info, id: int) -> Optional[Post]:
    """Get a single post by ID.

    This query is registered with TurboRouter.
    """
    db = info.context["db"]
    return await db.find_one("v_posts", id=id)


async def posts(info, limit: int = 10, offset: int = 0, user_id: Optional[int] = None) -> list[Post]:
    """Get a list of posts.

    This query is registered with TurboRouter.
    """
    db = info.context["db"]
    filters = {}
    if user_id is not None:
        filters["user_id"] = user_id

    return await db.find("v_posts", limit=limit, offset=offset, **filters)
