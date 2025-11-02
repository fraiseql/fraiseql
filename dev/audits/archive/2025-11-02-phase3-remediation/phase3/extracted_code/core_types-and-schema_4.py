# Extracted from: docs/core/types-and-schema.md
# Block number: 4
from uuid import UUID

from fraiseql import type


@type(sql_source="users", jsonb_column=None)
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime
