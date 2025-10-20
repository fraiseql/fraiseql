"""SHA-256 hashing utilities for audit event chains.

FraiseQL Philosophy: "In PostgreSQL Everything"
This module is for VERIFICATION ONLY. Event hashing during creation is handled
by PostgreSQL triggers (see migrations/001_audit_tables.sql).

Use this module to:
- Verify chain integrity from Python
- Calculate expected hashes for testing
- Audit/forensic analysis

DO NOT use this module to:
- Hash events during creation (PostgreSQL does this)
- Populate crypto fields manually (triggers handle this)
"""

import hashlib
import json
from typing import Any, Optional


def hash_audit_event(event_data: dict[str, Any], previous_hash: Optional[str]) -> str:
    """Generate SHA-256 hash of audit event linked to previous hash.

    NOTE: This is for VERIFICATION ONLY. Production event creation uses
    PostgreSQL's generate_event_hash() function via triggers.

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
