# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 8
from dataclasses import dataclass
from datetime import datetime
from typing import Any


@dataclass
class DomainEvent:
    """Base domain event."""

    event_type: str
    aggregate_id: str
    payload: dict[str, Any]
    timestamp: datetime = field(default_factory=datetime.utcnow)


# Orders Context: Publish event
from uuid import UUID

from fraiseql import mutation


@mutation
async def submit_order(info, order_id: UUID) -> Order:
    """Submit order and publish event."""
    order_repo = get_order_repository()
    order = await order_repo.get_by_id(order_id)
    order.submit()
    await order_repo.save(order)

    # Publish event for other contexts
    event = DomainEvent(
        event_type="OrderSubmitted",
        aggregate_id=order.id,
        payload={
            "order_id": order.id,
            "customer_id": order.customer_id,
            "total": str(order.total),
            "items": [
                {"product_id": item.product_id, "quantity": item.quantity} for item in order.items
            ],
        },
    )
    await publish_event(event)

    return order


# Billing Context: Subscribe to event
async def handle_order_submitted(event: DomainEvent):
    """Handle OrderSubmitted event from Orders context."""
    if event.event_type != "OrderSubmitted":
        return

    # Create invoice
    invoice = Invoice(
        id=str(uuid4()),
        order_id=event.payload["order_id"],
        customer_id=event.payload["customer_id"],
        amount=Decimal(event.payload["total"]),
        status="pending",
    )

    invoice_repo = get_invoice_repository()
    await invoice_repo.save(invoice)
