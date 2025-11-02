# Extracted from: docs/advanced/event-sourcing.md
# Block number: 2
from fraiseql import query, type_


@type_
class EntityChange:
    id: int
    entity_type: str
    entity_id: str
    operation: str
    changed_by: str | None
    changed_at: datetime
    before_snapshot: dict | None
    after_snapshot: dict | None
    changed_fields: dict | None


@query
async def get_order_history(info, order_id: str) -> list[EntityChange]:
    """Get complete audit trail for an order."""
    repo = EntityChangeLogRepository(get_db_pool())
    return await repo.get_entity_history("orders.orders", order_id)


@query
async def get_order_at_time(info, order_id: str, at_time: datetime) -> dict | None:
    """Get order state at specific point in time."""
    repo = EntityChangeLogRepository(get_db_pool())
    return await repo.get_entity_at_time("orders.orders", order_id, at_time)


@query
async def get_user_activity(info, user_id: str, limit: int = 50) -> list[EntityChange]:
    """Get all changes made by a user."""
    repo = EntityChangeLogRepository(get_db_pool())
    return await repo.get_changes_by_user(user_id, limit)
