"""Authentication module for FraiseQL."""

from fraiseql.auth.base import AuthProvider, UserContext
from fraiseql.auth.decorators import requires_auth, requires_permission, requires_role

__all__ = [
    "AuthProvider",
    "UserContext",
    "requires_auth",
    "requires_permission",
    "requires_role",
]
