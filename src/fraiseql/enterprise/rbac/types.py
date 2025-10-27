"""GraphQL types for FraiseQL Enterprise RBAC (Role-Based Access Control)."""

from datetime import datetime
from typing import Optional
from uuid import UUID

from fraiseql.strawberry_compat import strawberry
from fraiseql.types.scalars.json import JSONField

from .models import Permission, Role, UserRole


@strawberry.type
class RoleType:
    """GraphQL type for Role entity with hierarchical support."""

    id: UUID
    name: str
    description: Optional[str]
    parent_role_id: Optional[UUID]
    tenant_id: Optional[UUID]
    is_system: bool
    created_at: datetime
    updated_at: datetime

    @classmethod
    def from_model(cls, role: Role) -> "RoleType":
        """Create RoleType from Role model."""
        return cls(
            id=role.id,
            name=role.name,
            description=role.description,
            parent_role_id=role.parent_role_id,
            tenant_id=role.tenant_id,
            is_system=role.is_system,
            created_at=role.created_at,
            updated_at=role.updated_at,
        )


@strawberry.type
class PermissionType:
    """GraphQL type for Permission entity."""

    id: UUID
    resource: str
    action: str
    description: Optional[str]
    constraints: Optional[JSONField]
    created_at: datetime

    @classmethod
    def from_model(cls, permission: Permission) -> "PermissionType":
        """Create PermissionType from Permission model."""
        return cls(
            id=permission.id,
            resource=permission.resource,
            action=permission.action,
            description=permission.description,
            constraints=permission.constraints,
            created_at=permission.created_at,
        )


@strawberry.type
class UserRoleType:
    """GraphQL type for User-Role assignment."""

    id: UUID
    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID]
    granted_by: Optional[UUID]
    granted_at: datetime
    expires_at: Optional[datetime]

    @classmethod
    def from_model(cls, user_role: UserRole) -> "UserRoleType":
        """Create UserRoleType from UserRole model."""
        return cls(
            id=user_role.id,
            user_id=user_role.user_id,
            role_id=user_role.role_id,
            tenant_id=user_role.tenant_id,
            granted_by=user_role.granted_by,
            granted_at=user_role.granted_at,
            expires_at=user_role.expires_at,
        )


@strawberry.type
class RoleWithPermissions:
    """Role with its associated permissions."""

    role: RoleType
    permissions: list[PermissionType]
    inherited_permissions: list[PermissionType]


@strawberry.type
class UserPermissions:
    """User's effective permissions across all roles."""

    user_id: UUID
    tenant_id: Optional[UUID]
    permissions: list[PermissionType]
    roles: list[RoleType]


@strawberry.input
class RoleFilter:
    """Filter for querying roles."""

    tenant_id: Optional[UUID] = None
    name_contains: Optional[str] = None
    is_system: Optional[bool] = None


@strawberry.input
class PermissionFilter:
    """Filter for querying permissions."""

    resource: Optional[str] = None
    action: Optional[str] = None


@strawberry.input
class UserRoleFilter:
    """Filter for querying user roles."""

    user_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None
    role_id: Optional[UUID] = None


@strawberry.input
class AssignRoleInput:
    """Input for assigning a role to a user."""

    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None
    expires_at: Optional[datetime] = None


@strawberry.input
class RevokeRoleInput:
    """Input for revoking a role from a user."""

    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None


@strawberry.input
class CreateRoleInput:
    """Input for creating a new role."""

    name: str
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None


@strawberry.input
class UpdateRoleInput:
    """Input for updating an existing role."""

    id: UUID
    name: Optional[str] = None
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None


@strawberry.input
class CreatePermissionInput:
    """Input for creating a new permission."""

    resource: str
    action: str
    description: Optional[str] = None
    constraints: Optional[JSONField] = None


@strawberry.input
class PermissionCheckInput:
    """Input for checking user permissions."""

    user_id: UUID
    resource: str
    action: str
    tenant_id: Optional[UUID] = None


@strawberry.type
class PermissionCheckResult:
    """Result of a permission check."""

    has_permission: bool
    user_id: UUID
    resource: str
    action: str
    tenant_id: Optional[UUID]


@strawberry.type
class MutationResult:
    """Generic result for mutations."""

    success: bool
    message: str
    id: Optional[UUID] = None
