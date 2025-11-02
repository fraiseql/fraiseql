# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 5
# tests/integration/enterprise/audit/test_chain_builder.py


def test_event_hash_generation():
    """Verify event hash is deterministic and collision-resistant."""
    from fraiseql.enterprise.crypto.hashing import hash_audit_event

    event_data = {
        "event_type": "user.login",
        "user_id": "123e4567-e89b-12d3-a456-426614174000",
        "timestamp": "2025-01-15T10:30:00Z",
        "ip_address": "192.168.1.100",
        "data": {"method": "password"},
    }

    hash1 = hash_audit_event(event_data, previous_hash=None)
    hash2 = hash_audit_event(event_data, previous_hash=None)

    assert hash1 == hash2  # Deterministic
    assert len(hash1) == 64  # SHA-256 hex digest
    assert hash1 != hash_audit_event({**event_data, "user_id": "different"})
    # Expected failure: hash_audit_event not implemented
