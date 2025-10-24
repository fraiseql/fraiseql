"""Admin Panel Data Models.

Defines GraphQL types for customer support, operations, and sales dashboards.
"""

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from uuid import UUID

import fraiseql
from fraiseql import fraise_field


@fraiseql.type
class CustomerInfo:
    """Customer information for support dashboard."""

    id: UUID = fraise_field(description="Customer unique identifier")
    email: str = fraise_field(description="Customer email address")
    name: str = fraise_field(description="Customer full name")
    created_at: datetime = fraise_field(description="Account creation date")
    subscription_status: str = fraise_field(description="Current subscription status")
    total_spent: Decimal = fraise_field(description="Total amount spent")
    ticket_count: int = fraise_field(description="Number of support tickets")


@fraiseql.type
class SupportTicket:
    """Customer support ticket."""

    id: UUID = fraise_field(description="Ticket unique identifier")
    customer_id: UUID = fraise_field(description="Customer who created ticket")
    subject: str = fraise_field(description="Ticket subject")
    status: str = fraise_field(description="Ticket status")
    priority: str = fraise_field(description="Ticket priority")
    assigned_to_id: UUID | None = fraise_field(description="Assigned support agent")
    created_at: datetime = fraise_field(description="Ticket creation date")
    updated_at: datetime = fraise_field(description="Last update timestamp")


@fraiseql.type
class Order:
    """Customer order information."""

    id: UUID = fraise_field(description="Order unique identifier")
    order_number: str = fraise_field(description="Human-readable order number")
    customer_id: UUID = fraise_field(description="Customer who placed order")
    total: Decimal = fraise_field(description="Order total amount")
    status: str = fraise_field(description="Order fulfillment status")
    created_at: datetime = fraise_field(description="Order creation date")
    shipped_at: datetime | None = fraise_field(description="Shipping date")
    delivered_at: datetime | None = fraise_field(description="Delivery date")


@fraiseql.type
class OrderItem:
    """Individual item in an order."""

    id: UUID = fraise_field(description="Order item unique identifier")
    order_id: UUID = fraise_field(description="Parent order ID")
    product_name: str = fraise_field(description="Product name")
    product_sku: str = fraise_field(description="Product SKU")
    quantity: int = fraise_field(description="Quantity ordered")
    unit_price: Decimal = fraise_field(description="Price per unit")
    total_price: Decimal = fraise_field(description="Line item total")


@fraiseql.type
class OperationsMetrics:
    """Real-time operations dashboard metrics."""

    pending_orders: int = fraise_field(description="Orders pending processing")
    processing_orders: int = fraise_field(description="Orders currently processing")
    shipped_today: int = fraise_field(description="Orders shipped today")
    average_fulfillment_time: float = fraise_field(description="Average fulfillment time in hours")
    low_stock_items: int = fraise_field(description="Products with low stock")
    out_of_stock_items: int = fraise_field(description="Products out of stock")
    today_revenue: Decimal = fraise_field(description="Today's revenue")
    month_revenue: Decimal = fraise_field(description="Current month revenue")
    order_accuracy: float = fraise_field(description="Order accuracy percentage")
    on_time_delivery_rate: float = fraise_field(description="On-time delivery rate")


@fraiseql.type
class SalesMetrics:
    """Sales team performance metrics."""

    rep_id: UUID = fraise_field(description="Sales representative ID")
    rep_name: str = fraise_field(description="Sales representative name")
    current_month_revenue: Decimal = fraise_field(description="Revenue this month")
    quota_attainment: float = fraise_field(description="Quota attainment percentage")
    deals_in_pipeline: int = fraise_field(description="Active deals count")
    deals_won_this_month: int = fraise_field(description="Deals closed this month")
    average_deal_size: Decimal = fraise_field(description="Average deal value")


@fraiseql.type
class Deal:
    """Sales deal/opportunity."""

    id: UUID = fraise_field(description="Deal unique identifier")
    company_name: str = fraise_field(description="Company name")
    contact_name: str = fraise_field(description="Primary contact name")
    contact_email: str = fraise_field(description="Primary contact email")
    stage: str = fraise_field(description="Deal stage in pipeline")
    amount: Decimal = fraise_field(description="Deal value")
    probability: int = fraise_field(description="Win probability percentage")
    expected_close_date: datetime = fraise_field(description="Expected close date")
    assigned_to_id: UUID = fraise_field(description="Sales rep ID")
    created_at: datetime = fraise_field(description="Deal creation date")
    updated_at: datetime = fraise_field(description="Last update timestamp")
    notes: str | None = fraise_field(description="Deal notes")


@fraiseql.type
class AdminUser:
    """Admin panel user."""

    id: UUID = fraise_field(description="Admin user unique identifier")
    email: str = fraise_field(description="Admin email address")
    name: str = fraise_field(description="Admin full name")
    role: str = fraise_field(description="Admin role")
    is_active: bool = fraise_field(description="Whether account is active")
    created_at: datetime = fraise_field(description="Account creation date")


@fraiseql.type
class AuditLogEntry:
    """Audit log entry for admin actions."""

    id: UUID = fraise_field(description="Log entry unique identifier")
    admin_user_id: UUID = fraise_field(description="Admin who performed action")
    action: str = fraise_field(description="Action type")
    target_type: str | None = fraise_field(description="Target entity type")
    target_id: UUID | None = fraise_field(description="Target entity ID")
    details: dict = fraise_field(description="Action details")
    ip_address: str | None = fraise_field(description="Admin IP address")
    created_at: datetime = fraise_field(description="Action timestamp")


# Input types for mutations
@fraiseql.input
class CustomerUpdateInput:
    """Input for updating customer information."""

    subscription_status: str | None = fraise_field(description="New subscription status")
    notes: str | None = fraise_field(description="Admin notes")


@fraiseql.input
class OrderStatusUpdateInput:
    """Input for updating order status."""

    order_id: UUID = fraise_field(description="Order to update")
    new_status: str = fraise_field(description="New order status")
    notes: str | None = fraise_field(description="Status change notes")


@fraiseql.input
class DealUpdateInput:
    """Input for updating deal information."""

    deal_id: UUID = fraise_field(description="Deal to update")
    stage: str | None = fraise_field(description="New deal stage")
    amount: Decimal | None = fraise_field(description="Updated deal value")
    probability: int | None = fraise_field(description="Win probability (0-100)")
    expected_close_date: datetime | None = fraise_field(description="Expected close")
    notes: str | None = fraise_field(description="Update notes")
