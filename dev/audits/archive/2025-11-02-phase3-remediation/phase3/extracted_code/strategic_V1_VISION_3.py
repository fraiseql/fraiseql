# Extracted from: docs/strategic/V1_VISION.md
# Block number: 3
class CommandRepository:
    """Thin wrapper - calls database functions"""

    async def execute(self, sql: str, *params) -> Any:
        return await self.db.fetchval(sql, *params)


class QueryRepository:
    """Reads from tv_* views"""

    async def find_one(self, view: str, id: UUID = None, identifier: str = None) -> dict:
        if id:
            return await self.db.fetchrow(f"SELECT data FROM {view} WHERE id = $1", id)
        if identifier:
            return await self.db.fetchrow(
                f"SELECT data FROM {view} WHERE identifier = $1", identifier
            )
