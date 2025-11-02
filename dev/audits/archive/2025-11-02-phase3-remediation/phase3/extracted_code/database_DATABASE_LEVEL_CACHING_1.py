# Extracted from: docs/database/DATABASE_LEVEL_CACHING.md
# Block number: 1
from fraiseql import query, type


@type(sql_source="mv_dashboard_stats", jsonb_column="top_users")
class DashboardStats:
    total_users: int
    total_posts: int
    posts_today: int
    avg_post_length: float
    top_users: list[dict]


@query
async def dashboard(info) -> DashboardStats:
    """Query materialized view (0.5ms)
    Rust transforms top_users JSONB (0.3ms)
    Total: 0.8ms (vs 150ms live query)

    190x speedup!
    """
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("mv_dashboard_stats")


# Refresh strategy: Cron job
# */5 * * * * psql -c "REFRESH MATERIALIZED VIEW CONCURRENTLY mv_dashboard_stats"
