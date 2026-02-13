"""
Multi-tenant Users Service Schema
Composite key federation with (organizationId, userId)
"""

from fraiseql import type, key, ID
from typing import Optional


@type
@key(fields=["organization_id", "user_id"])
class User:
    """
    User entity in multi-tenant system
    Composite key ensures data isolation per organization
    """
    organization_id: str
    user_id: str
    name: str
    email: str
    role: str


@type
class Organization:
    """Organization entity"""
    id: str
    name: str
    users: list[User]


@type
class Query:
    """Root query type"""

    def user(
        self,
        organization_id: str,
        user_id: str,
    ) -> Optional[User]:
        """Get user by organizationId and userId (composite key)"""
        pass

    def users(self, organization_id: str) -> list[User]:
        """Get all users in organization"""
        pass

    def organization(self, id: str) -> Optional[Organization]:
        """Get organization by ID"""
        pass

    def organizations(self) -> list[Organization]:
        """Get all organizations"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_user(
        self,
        organization_id: str,
        user_id: str,
        name: str,
        email: str,
        role: str,
    ) -> User:
        """Create user in organization"""
        pass

    def update_user(
        self,
        organization_id: str,
        user_id: str,
        name: Optional[str] = None,
        email: Optional[str] = None,
        role: Optional[str] = None,
    ) -> Optional[User]:
        """Update user (composite key)"""
        pass

    def delete_user(
        self,
        organization_id: str,
        user_id: str,
    ) -> bool:
        """Delete user from organization"""
        pass

    def create_organization(self, id: str, name: str) -> Organization:
        """Create organization"""
        pass
