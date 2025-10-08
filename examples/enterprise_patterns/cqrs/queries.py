"""GraphQL Query Resolvers (Read Side).

All queries use optimized views (v_*) for fast data retrieval.
No business logic here - views are pre-computed and denormalized.
"""

from typing import Optional
from types import (
    Customer,
    Product,
    ProductInventory,
    OrderSummary,
    OrderItemDetails,
    Payment,
    RevenueByProduct,
    CustomerLifetimeValue,
    AuditLog,
    OrderStatusTimeline,
)


# ============================================================================
# CUSTOMER QUERIES
# ============================================================================


async def customer(info, id: int) -> Optional[Customer]:
    """Get a single customer by ID."""
    db = info.context["db"]
    return await db.find_one("v_customers", id=id)


async def customers(
    info,
    limit: int = 100,
    offset: int = 0,
    country: Optional[str] = None,
) -> list[Customer]:
    """Get a list of customers."""
    db = info.context["db"]
    filters = {}
    if country is not None:
        filters["country"] = country

    return await db.find(
        "v_customers",
        limit=limit,
        offset=offset,
        order_by="created_at DESC",
        **filters
    )


# ============================================================================
# PRODUCT QUERIES
# ============================================================================


async def product(info, id: int) -> Optional[Product]:
    """Get a single product by ID."""
    db = info.context["db"]
    return await db.find_one("v_products", id=id)


async def products(
    info,
    limit: int = 100,
    offset: int = 0,
    is_active: Optional[bool] = None,
    stock_status: Optional[str] = None,
) -> list[Product]:
    """Get a list of products."""
    db = info.context["db"]
    filters = {}
    if is_active is not None:
        filters["is_active"] = is_active
    if stock_status is not None:
        filters["stock_status"] = stock_status

    return await db.find(
        "v_products",
        limit=limit,
        offset=offset,
        order_by="name",
        **filters
    )


async def product_inventory(info) -> list[ProductInventory]:
    """Get real-time product inventory status."""
    db = info.context["db"]
    return await db.find("v_product_inventory", order_by="product_name")


# ============================================================================
# ORDER QUERIES
# ============================================================================


async def order(info, id: int) -> Optional[OrderSummary]:
    """Get a single order by ID (denormalized summary)."""
    db = info.context["db"]
    return await db.find_one("v_orders_summary", id=id)


async def order_by_number(info, order_number: str) -> Optional[OrderSummary]:
    """Get a single order by order number."""
    db = info.context["db"]
    return await db.find_one("v_orders_summary", order_number=order_number)


async def orders_summary(
    info,
    limit: int = 100,
    offset: int = 0,
    status: Optional[str] = None,
    customer_id: Optional[int] = None,
) -> list[OrderSummary]:
    """Get a list of orders (denormalized with customer info)."""
    db = info.context["db"]
    filters = {}
    if status is not None:
        filters["status"] = status
    if customer_id is not None:
        filters["customer_id"] = customer_id

    return await db.find(
        "v_orders_summary",
        limit=limit,
        offset=offset,
        order_by="created_at DESC",
        **filters
    )


async def order_items_details(info, order_id: int) -> list[OrderItemDetails]:
    """Get order items with product details."""
    db = info.context["db"]
    return await db.find(
        "v_order_items_details",
        order_id=order_id,
        order_by="id"
    )


# ============================================================================
# PAYMENT QUERIES
# ============================================================================


async def payment(info, id: int) -> Optional[Payment]:
    """Get a single payment by ID."""
    db = info.context["db"]
    return await db.find_one("v_payments", id=id)


async def payments(
    info,
    limit: int = 100,
    offset: int = 0,
    order_id: Optional[int] = None,
    status: Optional[str] = None,
) -> list[Payment]:
    """Get a list of payments."""
    db = info.context["db"]
    filters = {}
    if order_id is not None:
        filters["order_id"] = order_id
    if status is not None:
        filters["status"] = status

    return await db.find(
        "v_payments",
        limit=limit,
        offset=offset,
        order_by="created_at DESC",
        **filters
    )


# ============================================================================
# ANALYTICS QUERIES
# ============================================================================


async def revenue_by_product(info) -> list[RevenueByProduct]:
    """Get revenue analytics by product."""
    db = info.context["db"]
    return await db.find(
        "v_revenue_by_product",
        order_by="total_revenue DESC"
    )


async def customer_lifetime_value(
    info,
    limit: int = 100,
    offset: int = 0,
) -> list[CustomerLifetimeValue]:
    """Get customer lifetime value analytics."""
    db = info.context["db"]
    return await db.find(
        "v_customer_lifetime_value",
        limit=limit,
        offset=offset,
        order_by="lifetime_value DESC"
    )


# ============================================================================
# AUDIT QUERIES
# ============================================================================


async def audit_log(
    info,
    limit: int = 100,
    offset: int = 0,
    entity_type: Optional[str] = None,
    entity_id: Optional[int] = None,
) -> list[AuditLog]:
    """Get audit log entries."""
    db = info.context["db"]
    filters = {}
    if entity_type is not None:
        filters["entity_type"] = entity_type
    if entity_id is not None:
        filters["entity_id"] = entity_id

    return await db.find(
        "v_audit_log",
        limit=limit,
        offset=offset,
        order_by="created_at DESC",
        **filters
    )


async def order_status_timeline(info, order_id: int) -> Optional[OrderStatusTimeline]:
    """Get order status timeline with time metrics."""
    db = info.context["db"]
    return await db.find_one("v_order_status_timeline", order_id=order_id)


# ============================================================================
# NESTED RESOLVERS
# ============================================================================


async def Customer_orders(customer: Customer, info) -> list[OrderSummary]:
    """Get orders for a customer."""
    db = info.context["db"]
    return await db.find(
        "v_orders_summary",
        customer_id=customer.id,
        order_by="created_at DESC"
    )


async def OrderSummary_customer(order: OrderSummary, info) -> Optional[Customer]:
    """Get customer for an order."""
    db = info.context["db"]
    return await db.find_one("v_customers", id=order.customer_id)


async def OrderSummary_items(order: OrderSummary, info) -> list[OrderItemDetails]:
    """Get items for an order."""
    db = info.context["db"]
    return await db.find("v_order_items_details", order_id=order.id)


async def OrderSummary_payments(order: OrderSummary, info) -> list[Payment]:
    """Get payments for an order."""
    db = info.context["db"]
    return await db.find("v_payments", order_id=order.id, order_by="created_at DESC")


async def OrderItemDetails_product(item: OrderItemDetails, info) -> Optional[Product]:
    """Get product for an order item."""
    db = info.context["db"]
    return await db.find_one("v_products", id=item.product_id)
