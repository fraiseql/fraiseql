# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 4
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
