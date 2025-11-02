# Extracted from: docs/core/queries-and-mutations.md
# Block number: 6
from fraiseql import query


@query
async def get_user_stats(info, user_id: UUID) -> UserStats:
    repo = info.context["repo"]
    # Custom SQL query for complex aggregations
    # Exclusive Rust pipeline handles result processing automatically
    result = await repo.execute_raw(
        "SELECT count(*) as post_count FROM posts WHERE user_id = $1", user_id
    )
    return UserStats(post_count=result[0]["post_count"])
