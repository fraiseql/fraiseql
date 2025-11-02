# Extracted from: docs/core/types-and-schema.md
# Block number: 10
from datetime import datetime
from uuid import UUID

from fraiseql import type


@type
class User:
    id: UUID
    name: str
    role: UserRole


@type
class Order:
    id: UUID
    status: OrderStatus
    created_at: datetime
