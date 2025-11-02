# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 13
class AuditLogger:
    """Logs audit events with cryptographic chain and batching support."""

    def __init__(self, repo: FraiseQLRepository, batch_size: int = 100):
        self.repo = repo
        self.key_manager = get_key_manager()
        self.batch_size = batch_size
        self._batch: list[dict] = []

    async def log_event(
        self,
        event_type: str,
        event_data: dict[str, Any],
        user_id: Optional[str] = None,
        tenant_id: Optional[str] = None,
        ip_address: Optional[str] = None,
        immediate: bool = True,
    ) -> UUID:
        """Log audit event (batched or immediate).

        Args:
            event_type: Type of event
            event_data: Event data
            user_id: User ID
            tenant_id: Tenant ID
            ip_address: Source IP
            immediate: If True, write immediately; if False, batch

        Returns:
            UUID of event
        """
        event = self._prepare_event(event_type, event_data, user_id, tenant_id, ip_address)

        if immediate:
            return await self._write_event(event)
        self._batch.append(event)
        if len(self._batch) >= self.batch_size:
            await self.flush_batch()
        return event["id"]

    async def flush_batch(self):
        """Write all batched events to database."""
        if not self._batch:
            return

        # Write events in transaction
        async def write_batch(conn):
            for event in self._batch:
                await self._write_event(event, conn)

        await self.repo.run_in_transaction(write_batch)
        self._batch.clear()
