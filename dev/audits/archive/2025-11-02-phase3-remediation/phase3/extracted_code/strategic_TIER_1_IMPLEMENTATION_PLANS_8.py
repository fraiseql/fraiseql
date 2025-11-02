# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 8
def test_event_signature():
    """Verify HMAC-SHA256 signature prevents tampering."""
    from fraiseql.enterprise.crypto.signing import sign_event

    event_hash = "abc123def456"
    secret_key = "test-secret-key-do-not-use-in-production"

    signature = sign_event(event_hash, secret_key)

    assert len(signature) > 0
    assert signature == sign_event(event_hash, secret_key)  # Deterministic
    assert signature != sign_event(event_hash, "different-key")
    # Expected failure: sign_event not implemented
