# Extracted from: docs/advanced/event-sourcing.md
# Block number: 3
from datetime import datetime
from decimal import Decimal


class OrderEventReplayer:
    """Replay order events to rebuild state."""

    @staticmethod
    async def replay_to_state(entity_id: str, up_to_time: datetime | None = None) -> dict:
        """Replay events to rebuild order state."""
        repo = EntityChangeLogRepository(get_db_pool())

        async with repo.db.connection() as conn:
            query = """
                SELECT operation, after_snapshot, changed_at
                FROM audit.entity_change_log
                WHERE entity_type = 'orders.orders'
                  AND entity_id = $1
            """
            params = [entity_id]

            if up_to_time:
                query += " AND changed_at <= $2"
                params.append(up_to_time)

            query += " ORDER BY changed_at ASC"

            result = await conn.execute(query, *params)
            events = await result.fetchall()

        if not events:
            return None

        # Start with first event (INSERT)
        state = dict(events[0]["after_snapshot"])

        # Apply subsequent changes
        for event in events[1:]:
            if event["operation"] == "UPDATE":
                state.update(event["after_snapshot"])
            elif event["operation"] == "DELETE":
                return None  # Entity deleted

        return state

    @staticmethod
    async def rebuild_aggregate(entity_id: str) -> Order:
        """Rebuild complete Order aggregate from events."""
        state = await OrderEventReplayer.replay_to_state(entity_id)
        if not state:
            return None

        # Rebuild Order object
        order = Order(
            id=state["id"],
            customer_id=state["customer_id"],
            total=Decimal(str(state["total"])),
            status=state["status"],
            created_at=state["created_at"],
            updated_at=state["updated_at"],
        )

        # Rebuild order items from their change logs
        items_repo = EntityChangeLogRepository(get_db_pool())
        async with items_repo.db.connection() as conn:
            result = await conn.execute(
                """
                SELECT DISTINCT entity_id
                FROM audit.entity_change_log
                WHERE entity_type = 'orders.order_items'
                  AND (after_snapshot->>'order_id')::UUID = $1
            """,
                entity_id,
            )

            item_ids = [row["entity_id"] for row in await result.fetchall()]

        for item_id in item_ids:
            item_state = await OrderEventReplayer.replay_to_state(item_id)
            if item_state:  # Not deleted
                order.items.append(OrderItem(**item_state))

        return order
