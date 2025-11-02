# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 13
# src/fraiseql/enterprise/rbac/resolver.py

import logging
from uuid import UUID

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.enterprise.rbac.cache import PermissionCache
from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy
from fraiseql.enterprise.rbac.models import Permission, Role

logger = logging.getLogger(__name__)


class PermissionResolver:
    """Resolves effective permissions for users with PostgreSQL caching."""

    def __init__(self, repo: FraiseQLRepository, cache: PermissionCache | None = None):
        """Initialize permission resolver.

        Args:
            repo: FraiseQL database repository
            cache: Permission cache (optional, creates new if not provided)
        """
        self.repo = repo
        self.hierarchy = RoleHierarchy(repo)
        self.cache = cache or PermissionCache(repo.pool)

    async def get_user_permissions(
        self, user_id: UUID, tenant_id: UUID | None = None, use_cache: bool = True
    ) -> list[Permission]:
        """Get all effective permissions for a user.

        Flow:
        1. Check cache (request-level + PostgreSQL)
        2. If miss or stale, compute from database
        3. Cache result with domain versions
        4. Return permissions

        Args:
            user_id: User ID
            tenant_id: Optional tenant scope
            use_cache: Whether to use cache (default: True)

        Returns:
            List of effective permissions
        """
        # Try cache first
        if use_cache:
            cached = await self.cache.get(user_id, tenant_id)
            if cached is not None:
                logger.debug("Returning cached permissions for user %s", user_id)
                return cached

        # Cache miss or disabled - compute permissions
        logger.debug("Computing permissions for user %s", user_id)
        permissions = await self._compute_permissions(user_id, tenant_id)

        # Cache result
        if use_cache:
            await self.cache.set(user_id, tenant_id, permissions)

        return permissions

    async def _compute_permissions(self, user_id: UUID, tenant_id: UUID | None) -> list[Permission]:
        """Compute effective permissions from database.

        This is the expensive operation that we cache.

        Args:
            user_id: User ID
            tenant_id: Optional tenant scope

        Returns:
            List of effective permissions
        """
        # Get user's direct roles
        user_roles = await self._get_user_roles(user_id, tenant_id)

        # Get all inherited roles
        all_role_ids: set[UUID] = set()
        for role in user_roles:
            inherited = await self.hierarchy.get_inherited_roles(role.id)
            all_role_ids.update(r.id for r in inherited)

        if not all_role_ids:
            return []

        # Get permissions for all roles
        permissions_data = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT DISTINCT p.*
                FROM permissions p
                INNER JOIN role_permissions rp ON p.id = rp.permission_id
                WHERE rp.role_id = ANY(%s::uuid[])
                AND rp.granted = TRUE
                ORDER BY p.resource, p.action
            """,
                params={"role_ids": list(all_role_ids)},
                fetch_result=True,
            )
        )

        return [Permission(**row) for row in permissions_data]

    async def _get_user_roles(self, user_id: UUID, tenant_id: UUID | None) -> list[Role]:
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

    async def has_permission(
        self, user_id: UUID, resource: str, action: str, tenant_id: UUID | None = None
    ) -> bool:
        """Check if user has specific permission.

        Args:
            user_id: User ID
            resource: Resource name (e.g., 'user', 'product')
            action: Action name (e.g., 'create', 'read')
            tenant_id: Optional tenant scope

        Returns:
            True if user has permission, False otherwise
        """
        permissions = await self.get_user_permissions(user_id, tenant_id)

        return any(p.resource == resource and p.action == action for p in permissions)
