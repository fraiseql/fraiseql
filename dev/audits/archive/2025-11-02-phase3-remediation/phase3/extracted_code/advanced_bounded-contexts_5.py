# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 5
from uuid import UUID

from fraiseql import mutation, query


# Orders Context exports queries
@query
async def get_order(info, order_id: UUID) -> Order:
    """Orders context: Get order details."""
    order_repo = get_order_repository()
    return await order_repo.get_by_id(order_id)


# Billing Context consumes Orders data
@mutation
async def create_invoice_for_order(info, order_id: UUID) -> Invoice:
    """Billing context: Create invoice from order."""
    # Fetch order data via internal call or event
    order = await get_order(info, order_id)

    invoice = Invoice(
        id=str(uuid4()),
        order_id=order.id,
        customer_id=order.customer_id,
        amount=order.total,
        status="pending",
        due_date=datetime.utcnow() + timedelta(days=30),
    )

    invoice_repo = get_invoice_repository()
    return await invoice_repo.save(invoice)
