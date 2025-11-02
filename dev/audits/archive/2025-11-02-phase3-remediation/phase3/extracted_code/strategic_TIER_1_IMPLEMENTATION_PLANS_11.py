# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 11
# tests/integration/enterprise/audit/test_event_logger.py


async def test_log_audit_event():
    """Verify audit event is logged to database with proper chain."""
    from fraiseql.enterprise.audit.event_logger import AuditLogger

    logger = AuditLogger(db_repo)

    event_id = await logger.log_event(
        event_type="user.created",
        event_data={"username": "testuser", "email": "test@example.com"},
        user_id="123e4567-e89b-12d3-a456-426614174000",
        tenant_id="tenant-123",
        ip_address="192.168.1.100",
    )

    # Retrieve logged event
    events = await db_repo.run(
        DatabaseQuery(
            statement="SELECT * FROM audit_events WHERE id = %s",
            params={"id": event_id},
            fetch_result=True,
        )
    )

    assert len(events) == 1
    event = events[0]
    assert event["event_type"] == "user.created"
    assert event["event_hash"] is not None
    assert event["signature"] is not None
    # Expected failure: AuditLogger not implemented
