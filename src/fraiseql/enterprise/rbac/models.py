"""RBAC Data Models

Pydantic models for Role-Based Access Control entities.
These models represent the database schema and are used for type safety
and data validation throughout the RBAC system.
"""

from datetime import datetime
from typing import Any, Optional
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field


class Role(BaseModel):
    """Role entity with hierarchical support.

    Roles can inherit permissions from parent roles, supporting complex
    organizational structures with up to 10 levels of inheritance.
    """

    id: UUID
    name: str = Field(..., max_length=100)
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None  # NULL for global roles
    is_system: bool = False  # System roles can't be deleted
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class Permission(BaseModel):
    """Permission entity defining resource-action pairs.

    Permissions represent specific actions that can be performed on resources.
    They can include optional constraints for fine-grained access control.
    """

    id: UUID
    resource: str = Field(..., max_length=100)  # e.g., 'user', 'product', 'order'
    action: str = Field(..., max_length=50)  # e.g., 'create', 'read', 'update', 'delete'
    description: Optional[str] = None
    constraints: Optional[dict[str, Any]] = None  # JSONB constraints
    created_at: datetime

    model_config = ConfigDict(from_attributes=True)


class RolePermission(BaseModel):
    """Many-to-many mapping between roles and permissions.

    Links roles to permissions with explicit grant/revoke capability.
    """

    id: UUID
    role_id: UUID
    permission_id: UUID
    granted: bool = True  # TRUE = grant, FALSE = revoke (explicit deny)
    created_at: datetime

    model_config = ConfigDict(from_attributes=True)


class UserRole(BaseModel):
    """User-role assignment with tenant scoping and expiration.

    Assigns roles to users within specific tenants with optional expiration.
    """

    id: UUID
    user_id: UUID  # References users table
    role_id: UUID
    tenant_id: Optional[UUID] = None  # Scoped to tenant
    granted_by: Optional[UUID] = None  # User who granted this role
    granted_at: datetime
    expires_at: Optional[datetime] = None  # Optional expiration

    model_config = ConfigDict(from_attributes=True)


class PermissionCheck(BaseModel):
    """Input for permission checking operations."""

    user_id: UUID
    resource: str
    action: str
    tenant_id: Optional[UUID] = None


class RoleAssignment(BaseModel):
    """Input for role assignment operations."""

    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None
    granted_by: Optional[UUID] = None
    expires_at: Optional[datetime] = None
