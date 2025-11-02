# Extracted from: docs/tutorials/INTERACTIVE_EXAMPLES.md
# Block number: 1
from datetime import datetime
from uuid import UUID

from fraiseql import type


@type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime
