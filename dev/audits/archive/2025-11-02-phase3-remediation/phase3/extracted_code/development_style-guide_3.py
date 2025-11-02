# Extracted from: docs/development/style-guide.md
# Block number: 3
from uuid import UUID

from fraiseql import type


@type(sql_source="v_user")  # Always specify source for queryable types
class User:
    id: UUID  # Always use UUID not str for IDs
    name: str
    email: str
    created_at: str  # ISO format datetime strings
