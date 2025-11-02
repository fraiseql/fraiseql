# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 4
from fraiseql import type


# Add new field - no migration needed!
@type(sql_source="v_user")
class User:
    """User account.

    Fields:
        id: User identifier
        email: Email address
        name: Full name
        preferences: User preferences (NEW! Just add it)
    """

    id: UUID
    email: str
    name: str
    preferences: UserPreferences | None = None  # Added without ALTER TABLE
