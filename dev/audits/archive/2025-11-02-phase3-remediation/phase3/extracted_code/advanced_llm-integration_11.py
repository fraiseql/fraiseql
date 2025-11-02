# Extracted from: docs/advanced/llm-integration.md
# Block number: 11
from uuid import UUID

from fraiseql import query, type


@type(sql_source="v_user")
class User:
    """User account with profile information and order history.

    Users are created during registration and can place orders,
    manage their profile, and view order history.

    Fields:
        id: Unique user identifier (UUID format)
        email: User's email address (used for login)
        name: User's full name
        created_at: Account creation timestamp
        orders: All orders placed by this user, sorted by creation date descending
    """

    id: UUID
    email: str
    name: str
    created_at: datetime
    orders: list["Order"]


@query
async def user(info, id: UUID) -> User | None:
    """Get a single user by ID.

    Args:
        id: User UUID (format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)

    Returns:
        User object with all profile fields, or null if not found.

    Example:
        query {
          user(id: "123e4567-e89b-12d3-a456-426614174000") {
            id
            name
            email
          }
        }
    """
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": id})
