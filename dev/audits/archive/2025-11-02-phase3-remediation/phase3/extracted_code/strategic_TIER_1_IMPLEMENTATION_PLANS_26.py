# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 26
# src/fraiseql/enterprise/rbac/types.py

from typing import Optional
from uuid import UUID

import strawberry


@strawberry.type
class Role:
    """Role in RBAC system."""

    id: UUID
    name: str
    description: Optional[str]
    parent_role: Optional["Role"]
    permissions: list["Permission"]
    user_count: int

    @strawberry.field
    async def inherited_permissions(self, info: Info) -> list["Permission"]:
        """Get all permissions including inherited from parent roles."""
        from fraiseql.enterprise.rbac.resolver import PermissionResolver

        resolver = PermissionResolver(info.context["repo"])
        return await resolver.get_role_permissions(self.id, include_inherited=True)


@strawberry.type
class Permission:
    """Permission for resource action."""

    id: UUID
    resource: str
    action: str
    description: Optional[str]
    constraints: Optional[strawberry.scalars.JSON]


@strawberry.input
class CreateRoleInput:
    """Input for creating a role."""

    name: str
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    permission_ids: list[UUID] = strawberry.field(default_factory=list)


@strawberry.type
class RBACQuery:
    """GraphQL queries for RBAC."""

    @strawberry.field
    async def roles(self, info: Info, tenant_id: Optional[UUID] = None) -> list[Role]:
        """List all roles."""
        repo = info.context["repo"]
        results = await repo.run(
            DatabaseQuery(
                statement="""
                SELECT * FROM roles
                WHERE tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL)
                ORDER BY name
            """,
                params={"tenant_id": str(tenant_id) if tenant_id else None},
                fetch_result=True,
            )
        )
        return [Role(**row) for row in results]

    @strawberry.field
    async def permissions(self, info: Info) -> list[Permission]:
        """List all permissions."""
        repo = info.context["repo"]
        results = await repo.run(
            DatabaseQuery(
                statement="SELECT * FROM permissions ORDER BY resource, action",
                params={},
                fetch_result=True,
            )
        )
        return [Permission(**row) for row in results]

    @strawberry.field
    async def user_roles(self, info: Info, user_id: UUID) -> list[Role]:
        """Get roles assigned to a user."""
        from fraiseql.enterprise.rbac.resolver import PermissionResolver

        resolver = PermissionResolver(info.context["repo"])
        return await resolver.get_user_roles(user_id)
