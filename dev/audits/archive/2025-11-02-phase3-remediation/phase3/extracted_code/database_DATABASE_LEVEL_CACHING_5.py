# Extracted from: docs/database/DATABASE_LEVEL_CACHING.md
# Block number: 5
from fraiseql import query, type
from fraiseql.repositories import Repository


@type(sql_source="users", jsonb_column="data")
class User:
    id: int
    first_name: str
    last_name: str
    email: str
    active: bool
    user_posts: list[Post] | None = None


@type(sql_source="mv_dashboard")
class Dashboard:
    total_users: int
    new_users_week: int
    stats: dict


# Simple query - uses generated column
@query
async def user(info, id: int) -> User:
    """Pipeline:
    1. SELECT data FROM users WHERE id = $1 (0.05ms - partial index)
    2. Rust transform (0.5ms)
    Total: 0.55ms
    """
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("users", id=id)


# Dashboard - uses materialized view
@query
async def dashboard(info) -> Dashboard:
    """Pipeline:
    1. SELECT * FROM mv_dashboard (0.1ms - cached)
    2. Rust transform (0.3ms)
    Total: 0.4ms (vs 150ms without MV!)

    375x speedup!
    """
    repo = Repository(info.context["db"], info.context)
    result = await repo.db.fetchrow("SELECT * FROM mv_dashboard")
    return fraiseql_rs.transform_one(result, "Dashboard", info)
