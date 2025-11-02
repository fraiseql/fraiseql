# Extracted from: docs/advanced/advanced-patterns.md
# Block number: 9
class QueryRepository:
    async def find_one(
        self,
        view: str,
        id: UUID | None = None,  # By public UUID
        identifier: str | None = None,  # By human identifier
    ) -> dict | None:
        """Find by UUID or identifier"""
        if id:
            where = "id = $1"
            param = id
        elif identifier:
            where = "identifier = $1"
            param = identifier
        else:
            raise ValueError("Must provide id or identifier")

        result = await self.db.fetchrow(f"SELECT data FROM {view} WHERE {where}", param)
        return result["data"] if result else None

    async def find_by_identifier(self, view: str, identifier: str) -> dict | None:
        """Convenience method"""
        return await self.find_one(view, identifier=identifier)
