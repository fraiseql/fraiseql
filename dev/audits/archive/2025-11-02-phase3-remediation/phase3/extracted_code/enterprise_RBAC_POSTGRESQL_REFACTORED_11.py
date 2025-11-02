# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 11
# src/fraiseql/enterprise/rbac/hierarchy.py

from uuid import UUID

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.enterprise.rbac.models import Role


class RoleHierarchy:
    """Computes role hierarchy and inheritance."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo

    async def get_inherited_roles(self, role_id: UUID) -> list[Role]:
        """Get all roles in inheritance chain (including self).

        Uses PostgreSQL recursive CTE for efficient computation.

        Args:
            role_id: Starting role ID

        Returns:
            List of roles from most specific to most general

        Raises:
            ValueError: If cycle detected
        """
        results = await self.repo.run(
            DatabaseQuery(
                statement="SELECT * FROM get_inherited_roles(%s)",
                params={"role_id": str(role_id)},
                fetch_result=True,
            )
        )

        if not results:
            return []

        # Check if we hit cycle detection limit
        if any(r["depth"] >= 10 for r in results):
            raise ValueError(f"Cycle detected in role hierarchy for role {role_id}")

        # Get full role details
        role_ids = [r["role_id"] for r in results]
        roles_data = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT * FROM roles
                WHERE id = ANY(%s::uuid[])
                ORDER BY name
            """,
                params={"ids": role_ids},
                fetch_result=True,
            )
        )

        return [Role(**row) for row in roles_data]
