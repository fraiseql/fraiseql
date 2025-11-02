# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 9
# src/fraiseql/enterprise/crypto/signing.py

import hashlib
import hmac


def sign_event(event_hash: str, secret_key: str) -> str:
    """Generate HMAC-SHA256 signature for event hash.

    Args:
        event_hash: SHA-256 hash of event
        secret_key: Secret signing key

    Returns:
        Hex digest of HMAC signature
    """
    return hmac.new(
        key=secret_key.encode("utf-8"), msg=event_hash.encode("utf-8"), digestmod=hashlib.sha256
    ).hexdigest()


def verify_signature(event_hash: str, signature: str, secret_key: str) -> bool:
    """Verify HMAC signature matches event hash.

    Args:
        event_hash: SHA-256 hash of event
        signature: Claimed HMAC signature
        secret_key: Secret signing key

    Returns:
        True if signature is valid
    """
    expected_signature = sign_event(event_hash, secret_key)
    return hmac.compare_digest(signature, expected_signature)
