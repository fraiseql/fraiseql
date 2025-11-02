# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 18
# src/fraiseql/enterprise/audit/verification.py

from typing import Optional

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.enterprise.crypto.hashing import hash_audit_event
from fraiseql.enterprise.crypto.signing import get_key_manager


async def verify_chain(repo: FraiseQLRepository, tenant_id: Optional[str] = None) -> dict[str, Any]:
    """Verify integrity of audit event chain.

    Args:
        repo: Database repository
        tenant_id: Optional tenant filter

    Returns:
        Dictionary with verification results
    """
    # Retrieve all events in order
    events = await repo.run(
        DatabaseQuery(
            statement="""
            SELECT * FROM audit_events
            WHERE tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL)
            ORDER BY timestamp ASC
        """,
            params={"tenant_id": tenant_id},
            fetch_result=True,
        )
    )

    if not events:
        return {"valid": True, "total_events": 0, "broken_links": 0}

    key_manager = get_key_manager()
    broken_links = []
    previous_hash = None

    for event in events:
        # Verify hash links to previous event
        event_payload = {
            "event_type": event["event_type"],
            "event_data": event["event_data"],
            "user_id": str(event["user_id"]) if event["user_id"] else None,
            "tenant_id": str(event["tenant_id"]) if event["tenant_id"] else None,
            "timestamp": event["timestamp"].isoformat(),
            "ip_address": event["ip_address"],
        }

        expected_hash = hash_audit_event(event_payload, previous_hash)

        if expected_hash != event["event_hash"]:
            broken_links.append({"event_id": str(event["id"]), "reason": "hash_mismatch"})

        # Verify signature
        if not key_manager.verify(event["event_hash"], event["signature"]):
            broken_links.append({"event_id": str(event["id"]), "reason": "invalid_signature"})

        previous_hash = event["event_hash"]

    return {
        "valid": len(broken_links) == 0,
        "total_events": len(events),
        "broken_links": len(broken_links),
        "details": broken_links if broken_links else None,
    }
