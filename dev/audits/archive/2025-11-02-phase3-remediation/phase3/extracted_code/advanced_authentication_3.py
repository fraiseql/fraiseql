# Extracted from: docs/advanced/authentication.md
# Block number: 3
from graphql import GraphQLResolveInfo

from fraiseql import query


@query
async def get_my_profile(info: GraphQLResolveInfo) -> User:
    """Get current user's profile."""
    user_context = info.context["user"]
    if not user_context:
        raise AuthenticationError("Not authenticated")

    # user_context is UserContext instance
    return await fetch_user_by_id(user_context.user_id)
