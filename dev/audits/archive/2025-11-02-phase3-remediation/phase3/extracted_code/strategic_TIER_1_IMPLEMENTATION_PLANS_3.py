# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 3
# src/fraiseql/enterprise/audit/types.py

from datetime import datetime
from typing import Optional
from uuid import UUID

import strawberry


@strawberry.type
class AuditEvent:
    """Immutable audit log entry with cryptographic chain."""

    id: UUID
    event_type: str
    event_data: strawberry.scalars.JSON
    user_id: Optional[UUID]
    tenant_id: Optional[UUID]
    timestamp: datetime
    ip_address: Optional[str]
    previous_hash: Optional[str]
    event_hash: str
    signature: str

    @classmethod
    def from_db_row(cls, row: dict) -> "AuditEvent":
        """Create AuditEvent from database row."""
        return cls(
            id=row["id"],
            event_type=row["event_type"],
            event_data=row["event_data"],
            user_id=row.get("user_id"),
            tenant_id=row.get("tenant_id"),
            timestamp=row["timestamp"],
            ip_address=row.get("ip_address"),
            previous_hash=row.get("previous_hash"),
            event_hash=row["event_hash"],
            signature=row["signature"],
        )
