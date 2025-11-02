# Extracted from: docs/advanced/event-sourcing.md
# Block number: 4
from fraiseql import query


@query
async def get_order_timeline(
    info, order_id: str, from_time: datetime, to_time: datetime
) -> list[dict]:
    """Get order state snapshots over time."""
    repo = EntityChangeLogRepository(get_db_pool())

    async with repo.db.connection() as conn:
        result = await conn.execute(
            """
            SELECT
                changed_at,
                operation,
                after_snapshot,
                changed_by
            FROM audit.entity_change_log
            WHERE entity_type = 'orders.orders'
              AND entity_id = $1
              AND changed_at BETWEEN $2 AND $3
            ORDER BY changed_at ASC
        """,
            order_id,
            from_time,
            to_time,
        )

        return [dict(row) for row in await result.fetchall()]


@query
async def compare_states(info, order_id: str, time1: datetime, time2: datetime) -> dict:
    """Compare order state at two different times."""
    repo = EntityChangeLogRepository(get_db_pool())

    state1 = await repo.get_entity_at_time("orders.orders", order_id, time1)
    state2 = await repo.get_entity_at_time("orders.orders", order_id, time2)

    # Calculate diff
    changes = {}
    all_keys = set(state1.keys()) | set(state2.keys())

    for key in all_keys:
        val1 = state1.get(key)
        val2 = state2.get(key)
        if val1 != val2:
            changes[key] = {"from": val1, "to": val2}

    return {"state_at_time1": state1, "state_at_time2": state2, "changes": changes}
