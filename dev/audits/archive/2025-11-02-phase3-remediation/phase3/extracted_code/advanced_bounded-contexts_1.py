# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 1
from abc import ABC, abstractmethod
from uuid import UUID

from fraiseql.db import DatabasePool

T = TypeVar("T")


class Repository(ABC, Generic[T]):
    """Base repository for domain entities."""

    def __init__(self, db_pool: DatabasePool, schema: str = "public"):
        self.db = db_pool
        self.schema = schema
        self.table_name = self._get_table_name()

    @abstractmethod
    def _get_table_name(self) -> str:
        """Get table name for this repository."""

    async def get_by_id(self, id: UUID) -> T | None:
        """Get entity by ID."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                f"SELECT * FROM {self.schema}.{self.table_name} WHERE id = $1", id
            )
            row = await result.fetchone()
            return self._map_to_entity(row) if row else None

    async def get_all(self, limit: int = 100) -> list[T]:
        """Get all entities."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                f"SELECT * FROM {self.schema}.{self.table_name} LIMIT $1", limit
            )
            return [self._map_to_entity(row) for row in await result.fetchall()]

    async def save(self, entity: T) -> T:
        """Save entity (insert or update)."""
        # Implemented by subclasses
        raise NotImplementedError

    async def delete(self, id: UUID) -> bool:
        """Delete entity by ID."""
        async with self.db.connection() as conn:
            result = await conn.execute(
                f"DELETE FROM {self.schema}.{self.table_name} WHERE id = $1", id
            )
            return result.rowcount > 0

    @abstractmethod
    def _map_to_entity(self, row) -> T:
        """Map database row to entity."""
