# Extracted from: docs/development/style-guide.md
# Block number: 4
from fraiseql import query


@query
def get_users() -> list[User]:
    """Get all users."""
    # Implementation handled by framework


@query
def get_user_by_id(id: UUID) -> User:
    """Get a single user by ID."""
    # Implementation handled by framework
