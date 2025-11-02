# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 2
# tests/integration/enterprise/audit/test_audit_types.py


def test_audit_event_graphql_type():
    """Verify AuditEvent GraphQL type is properly defined."""
    schema = get_fraiseql_schema()

    audit_event_type = schema.type_map.get("AuditEvent")
    assert audit_event_type is not None

    fields = audit_event_type.fields
    assert "id" in fields
    assert "eventType" in fields
    assert "eventData" in fields
    assert "userId" in fields
    assert "timestamp" in fields
    assert "eventHash" in fields
    # Expected failure: AuditEvent type not defined yet
