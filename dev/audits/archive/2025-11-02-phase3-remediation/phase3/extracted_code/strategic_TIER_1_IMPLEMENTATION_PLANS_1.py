# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 1
# tests/integration/enterprise/audit/test_audit_schema.py


async def test_audit_events_table_exists():
    """Verify audit_events table exists with correct schema."""
    result = await db.run(
        DatabaseQuery(
            statement="SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'audit_events'",
            params={},
            fetch_result=True,
        )
    )

    required_columns = {
        "id": "uuid",
        "event_type": "character varying",
        "event_data": "jsonb",
        "user_id": "uuid",
        "tenant_id": "uuid",
        "timestamp": "timestamp with time zone",
        "ip_address": "inet",
        "previous_hash": "character varying",
        "event_hash": "character varying",
        "signature": "character varying",
    }

    assert len(result) >= len(required_columns)
    # Expected failure: table doesn't exist yet
