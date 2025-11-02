# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 6
# src/fraiseql/enterprise/crypto/hashing.py

import hashlib
import json
from typing import Any, Optional


def hash_audit_event(event_data: dict[str, Any], previous_hash: Optional[str]) -> str:
    """Generate SHA-256 hash of audit event linked to previous hash.

    Args:
        event_data: Event data to hash (must be JSON-serializable)
        previous_hash: Hash of previous event in chain (None for genesis event)

    Returns:
        64-character hex digest of SHA-256 hash
    """
    # Create canonical JSON representation (sorted keys for determinism)
    canonical_json = json.dumps(event_data, sort_keys=True, separators=(",", ":"))

    # Include previous hash in chain
    chain_data = f"{previous_hash or 'GENESIS'}:{canonical_json}"

    # Generate SHA-256 hash
    return hashlib.sha256(chain_data.encode("utf-8")).hexdigest()
