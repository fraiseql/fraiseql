"""
Users Service Schema
Owns the User entity in federation
"""

from fraiseql import type, key
from typing import Optional


@type
@key("id")
class User:
    """
    User entity
    Owned by users-service
    Can be extended by other subgraphs
    """
    id: str
    name: str
    email: str


@type
class Query:
    """Root query type"""

    def user(self, id: str) -> Optional[User]:
        """Get a single user by ID"""
        # FraiseQL automatically resolves from database
        pass

    def users(self) -> list[User]:
        """Get all users"""
        # FraiseQL automatically resolves from database
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_user(self, name: str, email: str) -> User:
        """Create a new user"""
        # FraiseQL automatically handles INSERT
        pass

    def update_user(self, id: str, name: Optional[str] = None, email: Optional[str] = None) -> Optional[User]:
        """Update a user"""
        # FraiseQL automatically handles UPDATE
        pass

    def delete_user(self, id: str) -> bool:
        """Delete a user"""
        # FraiseQL automatically handles DELETE
        pass
