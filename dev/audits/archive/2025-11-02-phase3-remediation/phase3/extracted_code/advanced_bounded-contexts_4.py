# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 4
from uuid import UUID

from graphql import GraphQLResolveInfo

from fraiseql import mutation


@mutation
async def create_order(info: GraphQLResolveInfo, customer_id: UUID) -> Order:
    """Create new order."""
    order = Order(customer_id=customer_id)
    order_repo = get_order_repository()
    return await order_repo.save(order)


@mutation
async def add_order_item(
    info: GraphQLResolveInfo, order_id: UUID, product_id: UUID, quantity: int, price: float
) -> Order:
    """Add item to order - enforces aggregate rules."""
    order_repo = get_order_repository()

    # Get aggregate
    order = await order_repo.get_by_id(order_id)
    if not order:
        raise ValueError("Order not found")

    # Modify through aggregate root
    order.add_item(product_id, quantity, Decimal(str(price)))

    # Save aggregate
    return await order_repo.save(order)


@mutation
async def submit_order(info: GraphQLResolveInfo, order_id: UUID) -> Order:
    """Submit order for processing."""
    order_repo = get_order_repository()

    order = await order_repo.get_by_id(order_id)
    if not order:
        raise ValueError("Order not found")

    # State transition through aggregate
    order.submit()

    return await order_repo.save(order)
