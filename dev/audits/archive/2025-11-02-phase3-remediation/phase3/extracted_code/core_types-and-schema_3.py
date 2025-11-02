# Extracted from: docs/core/types-and-schema.md
# Block number: 3
from uuid import UUID

from fraiseql import type


@type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
