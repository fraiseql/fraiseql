# Extracted from: docs/reference/quick-reference.md
# Block number: 1
from uuid import UUID

from fraiseql import type


@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str
    posts: list["Post"]  # Forward reference for relationships
