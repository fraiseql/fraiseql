# Extracted from: docs/advanced/event-sourcing.md
# Block number: 7
@dataclass
class VersionedEvent:
    """Event with schema version."""

    version: int
    event_type: str
    payload: dict


class EventUpgrader:
    """Upgrade old event schemas to current version."""

    @staticmethod
    def upgrade_order_created(event: dict, from_version: int) -> dict:
        """Upgrade OrderCreated event schema."""
        if from_version == 1:
            # v1 -> v2: Added customer_email
            event["customer_email"] = None
            from_version = 2

        if from_version == 2:
            # v2 -> v3: Added shipping_address
            event["shipping_address"] = None
            from_version = 3

        return event

    @staticmethod
    def upgrade_event(event: EntityChange) -> dict:
        """Upgrade event to current schema version."""
        current_version = 3
        event_version = event.metadata.get("schema_version", 1) if event.metadata else 1

        if event_version == current_version:
            return event.after_snapshot

        # Apply upgrades
        upgraded = dict(event.after_snapshot)
        if "OrderCreated" in event.entity_type:
            upgraded = EventUpgrader.upgrade_order_created(upgraded, event_version)

        return upgraded
