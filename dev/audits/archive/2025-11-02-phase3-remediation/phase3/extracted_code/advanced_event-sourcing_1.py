# Extracted from: docs/advanced/event-sourcing.md
# Block number: 1
from dataclasses import dataclass
from datetime import datetime
from typing import Any


@dataclass
class EntityChange:
    """Entity change event."""

    id: int
    entity_type: str
    entity_id: str
    operation: str
    changed_by: str | None
    changed_at: datetime
    before_snapshot: dict[str, Any] | None
    after_snapshot: dict[str, Any] | None
    changed_fields: dict[str, Any] | None
    metadata: dict[str, Any] | None
    transaction_id: int
    correlation_id: str | None


class EntityChangeLogRepository:
    """Repository for entity change logs."""

    def __init__(self, db_pool):
        self.db = db_pool

    async def get_entity_history(
        self, entity_type: str, entity_id: str, limit: int = 100
    ) -> list[EntityChange]:
        """Get complete history for an entity."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                """
                SELECT * FROM audit.entity_change_log
                WHERE entity_type = $1 AND entity_id = $2
                ORDER BY changed_at DESC
                LIMIT $3
            """,
                entity_type,
                entity_id,
                limit,
            )

            return [EntityChange(**row) for row in await result.fetchall()]

    async def get_changes_by_user(self, user_id: str, limit: int = 100) -> list[EntityChange]:
        """Get all changes made by a user."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                """
                SELECT * FROM audit.entity_change_log
                WHERE changed_by = $1
                ORDER BY changed_at DESC
                LIMIT $2
            """,
                user_id,
                limit,
            )

            return [EntityChange(**row) for row in await result.fetchall()]

    async def get_changes_in_transaction(self, transaction_id: int) -> list[EntityChange]:
        """Get all changes in a transaction."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                """
                SELECT * FROM audit.entity_change_log
                WHERE transaction_id = $1
                ORDER BY id
            """,
                transaction_id,
            )

            return [EntityChange(**row) for row in await result.fetchall()]

    async def get_entity_at_time(
        self, entity_type: str, entity_id: str, at_time: datetime
    ) -> dict[str, Any] | None:
        """Get entity state at specific point in time."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                """
                SELECT after_snapshot
                FROM audit.entity_change_log
                WHERE entity_type = $1
                  AND entity_id = $2
                  AND changed_at <= $3
                  AND operation != 'DELETE'
                ORDER BY changed_at DESC
                LIMIT 1
            """,
                entity_type,
                entity_id,
                at_time,
            )

            row = await result.fetchone()
            return row["after_snapshot"] if row else None
