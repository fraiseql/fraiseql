"""
Multi-Cloud Users Service (AWS us-east)
Owns User entity, not extended
"""

from fraiseql import type, key
from typing import Optional


@type
@key(fields=["id"])
class User:
    """User entity owned by users-service"""
    id: str
    name: str
    email: str
    created_at: str


@type
class Query:
    """Root query type"""

    def user(self, id: str) -> Optional[User]:
        """Get user by ID"""
        pass

    def users(self) -> list[User]:
        """Get all users"""
        pass

    def users_by_email(self, email: str) -> list[User]:
        """Get users by email (partial match)"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_user(self, id: str, name: str, email: str) -> User:
        """Create user"""
        pass

    def update_user(
        self,
        id: str,
        name: Optional[str] = None,
        email: Optional[str] = None,
    ) -> Optional[User]:
        """Update user"""
        pass

    def delete_user(self, id: str) -> bool:
        """Delete user"""
        pass
