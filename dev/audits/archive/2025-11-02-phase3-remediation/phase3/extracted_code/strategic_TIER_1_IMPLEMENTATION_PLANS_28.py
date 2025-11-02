# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 28
# src/fraiseql.enterprise/rbac/hierarchy.py

from typing import List
from uuid import UUID

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.enterprise.rbac.models import Role


class RoleHierarchy:
    """Computes role hierarchy and inheritance."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo

    async def get_inherited_roles(self, role_id: UUID) -> List[Role]:
        """Get all roles in inheritance chain (including self).

        Args:
            role_id: Starting role ID

        Returns:
            List of roles from most specific to most general
        """
        results = await self.repo.run(
            DatabaseQuery(
                statement="SELECT * FROM get_inherited_roles(%s)",
                params={"role_id": str(role_id)},
                fetch_result=True,
            )
        )

        # Get full role details
        role_ids = [r["role_id"] for r in results]
        roles = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT * FROM roles
                WHERE id = ANY(%s)
                ORDER BY name
            """,
                params={"ids": role_ids},
                fetch_result=True,
            )
        )

        return [Role(**row) for row in roles]
