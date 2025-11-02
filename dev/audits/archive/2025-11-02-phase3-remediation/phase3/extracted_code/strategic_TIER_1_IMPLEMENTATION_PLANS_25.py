# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 25
# src/fraiseql/enterprise/rbac/models.py

from dataclasses import dataclass
from datetime import datetime
from typing import Optional
from uuid import UUID


@dataclass
class Role:
    """Role with optional hierarchy."""

    id: UUID
    name: str
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None
    is_system: bool = False
    created_at: datetime = None
    updated_at: datetime = None


@dataclass
class Permission:
    """Permission for resource action."""

    id: UUID
    resource: str
    action: str
    description: Optional[str] = None
    constraints: Optional[dict] = None
    created_at: datetime = None


@dataclass
class UserRole:
    """User-Role assignment."""

    id: UUID
    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None
    granted_by: Optional[UUID] = None
    granted_at: datetime = None
    expires_at: Optional[datetime] = None
