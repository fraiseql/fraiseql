# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 6
from fraiseql import type


@type(sql_source="v_user")
class User:
    """User account with authentication and profile information.

    Users are created during registration and can access the system
    based on their assigned roles and permissions.

    Fields:
        id: Unique user identifier (UUID v4)
        email: Email address used for login (must be unique)
        first_name: User's first name
        last_name: User's last name
        created_at: Account creation timestamp
        is_active: Whether user account is active
    """

    id: UUID
    email: str
    first_name: str
    last_name: str
    created_at: datetime
    is_active: bool
