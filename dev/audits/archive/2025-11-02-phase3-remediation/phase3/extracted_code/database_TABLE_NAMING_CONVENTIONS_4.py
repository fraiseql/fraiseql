# Extracted from: docs/database/TABLE_NAMING_CONVENTIONS.md
# Block number: 4
from fraiseql import query, type


@type(sql_source="tv_user", jsonb_column="data")
class User:
    id: int
    first_name: str
    user_posts: list[Post] | None


@query
async def user(info, id: int) -> User:
    # Queries tv_user (0.05ms lookup + 0.5ms Rust transform = 0.55ms)
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("tv_user", id=id)
