# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 12
# src/fraiseql/enterprise/audit/event_logger.py

from datetime import datetime
from typing import Any, Optional
from uuid import UUID, uuid4

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.enterprise.crypto.hashing import hash_audit_event
from fraiseql.enterprise.crypto.signing import get_key_manager


class AuditLogger:
    """Logs audit events with cryptographic chain."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo
        self.key_manager = get_key_manager()

    async def log_event(
        self,
        event_type: str,
        event_data: dict[str, Any],
        user_id: Optional[str] = None,
        tenant_id: Optional[str] = None,
        ip_address: Optional[str] = None,
    ) -> UUID:
        """Log an audit event with cryptographic chain.

        Args:
            event_type: Type of event (e.g., 'user.login', 'data.modified')
            event_data: Event-specific data
            user_id: ID of user who triggered event
            tenant_id: Tenant context
            ip_address: Source IP address

        Returns:
            UUID of created audit event
        """
        # Get previous event hash for chain
        previous_hash = await self._get_latest_hash(tenant_id)

        # Create event payload
        timestamp = datetime.utcnow()
        event_payload = {
            "event_type": event_type,
            "event_data": event_data,
            "user_id": user_id,
            "tenant_id": tenant_id,
            "timestamp": timestamp.isoformat(),
            "ip_address": ip_address,
        }

        # Generate hash and signature
        event_hash = hash_audit_event(event_payload, previous_hash)
        signature = self.key_manager.sign(event_hash)

        # Insert into database
        event_id = uuid4()
        await self.repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO audit_events (
                    id, event_type, event_data, user_id, tenant_id,
                    timestamp, ip_address, previous_hash, event_hash, signature
                ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            """,
                params={
                    "id": event_id,
                    "event_type": event_type,
                    "event_data": event_data,
                    "user_id": user_id,
                    "tenant_id": tenant_id,
                    "timestamp": timestamp,
                    "ip_address": ip_address,
                    "previous_hash": previous_hash,
                    "event_hash": event_hash,
                    "signature": signature,
                },
                fetch_result=False,
            )
        )

        return event_id

    async def _get_latest_hash(self, tenant_id: Optional[str]) -> Optional[str]:
        """Get hash of most recent audit event in chain."""
        result = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT event_hash FROM audit_events
                WHERE tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL)
                ORDER BY timestamp DESC
                LIMIT 1
            """,
                params={"tenant_id": tenant_id},
                fetch_result=True,
            )
        )

        return result[0]["event_hash"] if result else None
