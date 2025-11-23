"""GraphQL Mutation Resolvers (Write Side).

All mutations use PostgreSQL functions to encapsulate business logic.
This provides:
- ACID guarantees
- Centralized validation
- Automatic audit logging
- Optimistic locking support
"""

import json
from types import (
    AddProductInput,
    CancelOrderInput,
    CreateOrderInput,
    OrderSummary,
    ProcessPaymentInput,
    Product,
    UpdateOrderStatusInput,
    UpdateProductInventoryInput,
)
from typing import Optional

# ============================================================================
# ORDER MUTATIONS
# ============================================================================


async def create_order(info, input: CreateOrderInput) -> OrderSummary:
    """Create a new order.

    Calls fn_create_order() which:
    - Validates customer exists
    - Validates products exist and have inventory
    - Creates order and order items atomically
    - Reserves inventory
    - Logs to audit trail

    Example:
        mutation {
            createOrder(input: {
                customerId: 1
                items: [
                    { productId: 1, quantity: 2 }
                    { productId: 3, quantity: 1 }
                ]
                notes: "Rush delivery"
            }) {
                id
                orderNumber
                total
                status
            }
        }
    """
    db = info.context["db"]

    # Get user from context for audit trail
    changed_by = info.context.get("user_id", "system")

    # Convert items to JSONB format for PostgreSQL
    items_json = json.dumps([
        {"product_id": item.product_id, "quantity": item.quantity}
        for item in input.items
    ])

    result = await db.execute_function(
        "fn_create_order",
        p_customer_id=input.customer_id,
        p_items=items_json,
        p_notes=input.notes,
        p_changed_by=changed_by,
    )

    if not result:
        raise Exception("Failed to create order")

    return result[0]


async def process_payment(info, input: ProcessPaymentInput) -> OrderSummary:
    """Process payment for an order.

    Calls fn_process_payment() which:
    - Validates order exists and is in 'pending' status
    - Validates payment amount matches order total
    - Checks optimistic lock version (if provided)
    - Creates payment record
    - Updates order status to 'paid'
    - Logs to audit trail

    Example:
        mutation {
            processPayment(input: {
                orderId: 1
                amount: 299.99
                paymentMethod: "credit_card"
                transactionId: "TXN-123"
                version: 1  # Optimistic locking
            }) {
                id
                orderNumber
                status
                paidAt
                version
            }
        }
    """
    db = info.context["db"]
    changed_by = info.context.get("user_id", "system")

    result = await db.execute_function(
        "fn_process_payment",
        p_order_id=input.order_id,
        p_amount=input.amount,
        p_payment_method=input.payment_method,
        p_transaction_id=input.transaction_id,
        p_version=input.version,
        p_changed_by=changed_by,
    )

    if not result:
        raise Exception("Failed to process payment")

    return result[0]


async def cancel_order(info, input: CancelOrderInput) -> OrderSummary:
    """Cancel an order.

    Calls fn_cancel_order() which:
    - Validates order can be cancelled (not shipped/delivered)
    - Processes refund if order was paid
    - Releases reserved inventory
    - Updates order status to 'cancelled'
    - Logs cancellation reason and audit trail

    Example:
        mutation {
            cancelOrder(input: {
                orderId: 3
                reason: "Customer request"
            }) {
                id
                orderNumber
                status
                cancelledAt
                cancellationReason
            }
        }
    """
    db = info.context["db"]
    changed_by = info.context.get("user_id", "system")

    result = await db.execute_function(
        "fn_cancel_order",
        p_order_id=input.order_id,
        p_reason=input.reason,
        p_changed_by=changed_by,
    )

    if not result:
        raise Exception("Failed to cancel order")

    # Return the full order summary
    return await db.find_one("v_orders_summary", id=input.order_id)


async def update_order_status(info, input: UpdateOrderStatusInput) -> OrderSummary:
    """Update order status.

    Calls fn_update_order_status() which:
    - Validates status transition is allowed
    - Updates status with appropriate timestamps
    - Releases reserved inventory and deducts from available (for shipped/delivered)
    - Logs to audit trail

    Example:
        mutation {
            updateOrderStatus(input: {
                orderId: 2
                newStatus: "shipped"
            }) {
                id
                orderNumber
                status
                shippedAt
            }
        }
    """
    db = info.context["db"]
    changed_by = info.context.get("user_id", "system")

    result = await db.execute_function(
        "fn_update_order_status",
        p_order_id=input.order_id,
        p_new_status=input.new_status,
        p_changed_by=changed_by,
    )

    if not result:
        raise Exception("Failed to update order status")

    return result[0]


# ============================================================================
# PRODUCT MUTATIONS
# ============================================================================


async def add_product(info, input: AddProductInput) -> Product:
    """Add a new product.

    Calls fn_add_product() which:
    - Validates SKU is unique
    - Creates product record
    - Logs to audit trail

    Example:
        mutation {
            addProduct(input: {
                sku: "LAPTOP-002"
                name: "Gaming Laptop"
                description: "High-end gaming laptop"
                price: 1899.99
                cost: 1200.00
                quantityAvailable: 25
            }) {
                id
                sku
                name
                price
            }
        }
    """
    db = info.context["db"]
    changed_by = info.context.get("user_id", "system")

    result = await db.execute_function(
        "fn_add_product",
        p_sku=input.sku,
        p_name=input.name,
        p_description=input.description,
        p_price=input.price,
        p_cost=input.cost,
        p_quantity_available=input.quantity_available,
        p_changed_by=changed_by,
    )

    if not result:
        raise Exception("Failed to add product")

    # Return full product details from view
    return await db.find_one("v_products", id=result[0].id)


async def update_product_inventory(info, input: UpdateProductInventoryInput) -> Product:
    """Update product inventory.

    Calls fn_update_product_inventory() which:
    - Validates product exists
    - Validates inventory won't go negative
    - Updates quantity_available
    - Logs to audit trail

    Example:
        mutation {
            updateProductInventory(input: {
                productId: 1
                quantityChange: 50  # Add 50 units
            }) {
                id
                sku
                name
                quantityAvailable
                quantityReserved
                quantityInStock
            }
        }

        # Remove inventory:
        mutation {
            updateProductInventory(input: {
                productId: 1
                quantityChange: -10  # Remove 10 units
            }) {
                ...
            }
        }
    """
    db = info.context["db"]
    changed_by = info.context.get("user_id", "system")

    result = await db.execute_function(
        "fn_update_product_inventory",
        p_product_id=input.product_id,
        p_quantity_change=input.quantity_change,
        p_changed_by=changed_by,
    )

    if not result:
        raise Exception("Failed to update product inventory")

    # Return full product details from view
    return await db.find_one("v_products", id=input.product_id)


# ============================================================================
# HELPER: Get Current User from Context
# ============================================================================


def get_current_user(info) -> str:
    """Extract current user from GraphQL context.

    In production, this would come from JWT token or session.
    """
    # Try to get user from various sources
    user_id = info.context.get("user_id")
    if user_id:
        return f"user:{user_id}"

    request = info.context.get("request")
    if request:
        # Try header
        user_header = request.headers.get("X-User-ID")
        if user_header:
            return f"user:{user_header}"

        # Try from authentication
        if hasattr(request, "user"):
            return f"user:{request.user.id}"

    return "system"
