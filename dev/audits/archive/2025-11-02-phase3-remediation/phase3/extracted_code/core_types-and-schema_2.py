# Extracted from: docs/core/types-and-schema.md
# Block number: 2
from datetime import datetime
from uuid import UUID

from fraiseql import type


@type
class User:
    id: UUID
    email: str
    name: str | None
    created_at: datetime
    is_active: bool = True
    tags: list[str] = []
