"""GraphQL types for FraiseQL Enterprise Audit Logging."""

from datetime import datetime
from uuid import UUID

from fraiseql.strawberry_compat import strawberry
from fraiseql.types.scalars.json import JSONField


@strawberry.type
class AuditEvent:
    """Immutable audit log entry with cryptographic chain."""

    id: UUID
    event_type: str
    event_data: JSONField
    user_id: UUID | None
    tenant_id: UUID | None
    timestamp: datetime
    ip_address: str | None
    previous_hash: str | None
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

    event_type: str | None = None
    user_id: UUID | None = None
    tenant_id: UUID | None = None
    start_time: datetime | None = None
    end_time: datetime | None = None


@strawberry.type
class AuditEventConnection:
    """Paginated audit events with chain metadata."""

    events: list[AuditEvent]
    total_count: int
    chain_valid: bool  # Result of integrity verification
    has_more: bool
