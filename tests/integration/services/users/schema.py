"""Users subgraph schema - owns User entity"""

from fraiseql import type, key, query, mutation, ID
from typing import Optional, List
from uuid import UUID


@type
@key(fields=["id"])
class User:
    """User entity - owned by this subgraph"""
    id: ID
    email: str
    name: str
    identifier: str


@type
class Query:
    """Root query type"""

    def user(self, id: ID) -> Optional[User]:
        """Get user by ID"""
        pass

    def users(self) -> List[User]:
        """Get all users"""
        pass

    def users_by_email(self, email: str) -> Optional[User]:
        """Get user by email"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_user(self, email: str, name: str) -> User:
        """Create a new user"""
        pass

    def update_user(
        self,
        id: ID,
        email: Optional[str] = None,
        name: Optional[str] = None
    ) -> Optional[User]:
        """Update user"""
        pass

    def delete_user(self, id: ID) -> bool:
        """Delete user"""
        pass
