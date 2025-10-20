"""GraphQL types for FraiseQL Enterprise Audit Logging."""

from datetime import datetime
from typing import Optional
from uuid import UUID

from fraiseql.strawberry_compat import strawberry
from fraiseql.types.scalars.json import JSONField


@strawberry.type
class AuditEvent:
    """Immutable audit log entry with cryptographic chain."""

    id: UUID
    event_type: str
    event_data: JSONField
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


@strawberry.input
class AuditEventFilter:
    """Filter for querying audit events."""

    event_type: Optional[str] = None
    user_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None
    start_time: Optional[datetime] = None
    end_time: Optional[datetime] = None


@strawberry.type
class AuditEventConnection:
    """Paginated audit events with chain metadata."""

    events: list[AuditEvent]
    total_count: int
    chain_valid: bool  # Result of integrity verification
    has_more: bool
