# Extracted from: docs/database/TABLE_NAMING_CONVENTIONS.md
# Block number: 3
from fraiseql import mutation, query, type


@type(sql_source="tv_user", jsonb_column="data")
class User:
    id: int
    first_name: str  # Rust transforms to firstName
    last_name: str  # Rust transforms to lastName
    email: str
    user_posts: list[Post] | None = None  # Embedded!


@query
async def user(info, id: int) -> User:
    # 1. SELECT data FROM tv_user WHERE id = $1 (0.05ms)
    # 2. Rust transform (0.5ms)
    # Total: 0.55ms (vs 5-10ms with v_user!)
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("tv_user", id=id)


@mutation
async def update_user(info, id: int, input: UpdateUserInput) -> User:
    # Update base table
    repo = Repository(info.context["db"], info.context)
    await repo.update("tb_user", input, id=id)

    # CRITICAL: Explicitly sync tv_user
    await repo.call_function("fn_sync_tv_user", {"p_id": id})

    # Return updated data
    return await repo.find_one("tv_user", id=id)
