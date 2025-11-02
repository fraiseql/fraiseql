# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 31
# src/fraiseql/enterprise/rbac/resolver.py

from typing import List, Set
from uuid import UUID

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy
from fraiseql.enterprise.rbac.models import Permission, Role


class PermissionResolver:
    """Resolves effective permissions for users."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo
        self.hierarchy = RoleHierarchy(repo)

    async def get_user_permissions(
        self, user_id: UUID, tenant_id: Optional[UUID] = None
    ) -> List[Permission]:
        """Get all effective permissions for a user.

        Computes permissions from all assigned roles and their parents.

        Args:
            user_id: User ID
            tenant_id: Optional tenant scope

        Returns:
            List of effective permissions
        """
        # Get user's direct roles
        user_roles = await self._get_user_roles(user_id, tenant_id)

        # Get all inherited roles
        all_role_ids: Set[UUID] = set()
        for role in user_roles:
            inherited = await self.hierarchy.get_inherited_roles(role.id)
            all_role_ids.update(r.id for r in inherited)

        if not all_role_ids:
            return []

        # Get permissions for all roles
        permissions = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT DISTINCT p.*
                FROM permissions p
                INNER JOIN role_permissions rp ON p.id = rp.permission_id
                WHERE rp.role_id = ANY(%s::uuid[])
                AND rp.granted = TRUE
            """,
                params={"role_ids": list(all_role_ids)},
                fetch_result=True,
            )
        )

        return [Permission(**row) for row in permissions]

    async def _get_user_roles(self, user_id: UUID, tenant_id: Optional[UUID]) -> List[Role]:
        """Get roles directly assigned to user."""
        results = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT r.*
                FROM roles r
                INNER JOIN user_roles ur ON r.id = ur.role_id
                WHERE ur.user_id = %s
                AND (ur.tenant_id = %s OR (ur.tenant_id IS NULL AND %s IS NULL))
                AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            """,
                params={
                    "user_id": str(user_id),
                    "tenant_id": str(tenant_id) if tenant_id else None,
                },
                fetch_result=True,
            )
        )

        return [Role(**row) for row in results]
