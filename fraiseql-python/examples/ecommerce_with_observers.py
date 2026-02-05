"""Example: E-commerce schema with observers.

This example demonstrates the observer authoring API in FraiseQL v2.
Observers react to database changes (INSERT, UPDATE, DELETE) with
configurable actions like webhooks, Slack notifications, and emails.

Usage:
    python examples/ecommerce_with_observers.py

Output:
    ecommerce_schema.json (ready for fraiseql-cli compilation)
"""

import fraiseql
from fraiseql import ID, DateTime, email, observer, slack, type, webhook
from fraiseql.observers import RetryConfig


# Define types
@type
class Order:
    """E-commerce order."""

    id: ID
    customer_email: str
    status: str
    total: float
    created_at: DateTime


@type
class Payment:
    """Payment record."""

    id: ID
    order_id: ID
    amount: float
    status: str
    processed_at: DateTime | None


# Observer 1: Notify when high-value orders are created
@observer(
    entity="Order",
    event="INSERT",
    condition="total > 1000",
    actions=[
        webhook("https://api.example.com/high-value-orders"),
        slack("#sales", "🎉 High-value order {id}: ${total}"),
        email(
            to="sales@example.com",
            subject="High-value order {id}",
            body="Order {id} for ${total} was created by {customer_email}",
        ),
    ],
)
def on_high_value_order():
    """Triggered when a high-value order is created."""
    pass


# Observer 2: Notify when orders are shipped
@observer(
    entity="Order",
    event="UPDATE",
    condition="status.changed() and status == 'shipped'",
    actions=[
        webhook(url_env="SHIPPING_WEBHOOK_URL"),
        email(
            to="{customer_email}",
            subject="Your order {id} has shipped!",
            body="Your order is on its way. Track it here: https://example.com/track/{id}",
            from_email="noreply@example.com",
        ),
    ],
)
def on_order_shipped():
    """Triggered when an order status changes to 'shipped'."""
    pass


# Observer 3: Alert on payment failures with aggressive retry
@observer(
    entity="Payment",
    event="UPDATE",
    condition="status == 'failed'",
    actions=[
        slack("#payments", "⚠️ Payment failed for order {order_id}: {amount}"),
        webhook(
            "https://api.example.com/payment-failures",
            headers={"Authorization": "Bearer {PAYMENT_API_TOKEN}"},
        ),
    ],
    retry=RetryConfig(
        max_attempts=5,
        backoff_strategy="exponential",
        initial_delay_ms=100,
        max_delay_ms=60000,
    ),
)
def on_payment_failure():
    """Triggered when a payment fails."""
    pass


# Observer 4: Archive deleted orders
@observer(
    entity="Order",
    event="DELETE",
    actions=[
        webhook(
            "https://api.example.com/archive",
            body_template='{"type": "order", "id": "{{id}}", "data": {{_json}}}',
        ),
    ],
)
def on_order_deleted():
    """Triggered when an order is deleted."""
    pass


# Observer 5: Simple notification for all new orders
@observer(
    entity="Order",
    event="INSERT",
    actions=[slack("#orders", "New order {id} by {customer_email}")],
)
def on_order_created():
    """Triggered when any order is created."""
    pass


if __name__ == "__main__":
    # Export schema with observers
    fraiseql.export_schema("ecommerce_schema.json")

    print("\n🎯 Observer Summary:")
    print("   1. on_high_value_order → Webhooks, Slack, Email for total > 1000")
    print("   2. on_order_shipped → Webhook + customer email when status='shipped'")
    print("   3. on_payment_failure → Slack + webhook with retry on payment failures")
    print("   4. on_order_deleted → Archive deleted orders via webhook")
    print("   5. on_order_created → Slack notification for all new orders")
    print("\n✨ Next steps:")
    print("   1. fraiseql-cli compile ecommerce_schema.json")
    print("   2. fraiseql-server --schema ecommerce_schema.compiled.json")
    print("   3. Observers will execute automatically on database changes!")
