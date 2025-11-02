# Extracted from: docs/core/queries-and-mutations.md
# Block number: 4
from graphql import GraphQLError

from fraiseql import query


@query
async def get_my_profile(info) -> User:
    user_context = info.context.get("user")
    if not user_context:
        raise GraphQLError("Authentication required")

    repo = info.context["repo"]
    # Exclusive Rust pipeline works with authentication automatically
    return await repo.find_one_rust("v_user", "user", info, id=user_context.user_id)
