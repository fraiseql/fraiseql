"""GraphQL Type Definitions for CQRS Example.

These types map to the read views (v_*) for queries.
Mutations use PostgreSQL functions and return these types.
"""

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from typing import Optional


# ============================================================================
# ENTITY TYPES (Read Model)
# ============================================================================


@dataclass
class Customer:
    """Customer entity from v_customers view."""

    id: int
    email: str
    name: str
    phone: Optional[str]
    address: Optional[str]
    city: Optional[str]
    country: Optional[str]
    created_at: datetime
    updated_at: datetime
    version: int

    # Relationships
    orders: Optional[list["OrderSummary"]] = None


@dataclass
class Product:
    """Product entity from v_products view."""

    id: int
    sku: str
    name: str
    description: Optional[str]
    price: Decimal
    cost: Decimal
    quantity_available: int
    quantity_reserved: int
    quantity_in_stock: int
    stock_status: str  # 'in_stock', 'low_stock', 'out_of_stock'
    is_active: bool
    created_at: datetime
    updated_at: datetime
    version: int


@dataclass
class ProductInventory:
    """Product inventory from v_product_inventory view."""

    product_id: int
    sku: str
    product_name: str
    quantity_available: int
    quantity_reserved: int
    quantity_in_stock: int
    quantity_in_orders: int
    low_stock: bool
    is_active: bool


@dataclass
class OrderSummary:
    """Order summary from v_orders_summary view (denormalized)."""

    id: int
    order_number: str
    customer_id: int
    customer_name: str
    customer_email: str
    customer_country: str
    status: str  # 'pending', 'paid', 'processing', 'shipped', 'delivered', 'cancelled'
    subtotal: Decimal
    tax: Decimal
    shipping: Decimal
    total: Decimal
    item_count: int
    notes: Optional[str]
    paid_at: Optional[datetime]
    shipped_at: Optional[datetime]
    delivered_at: Optional[datetime]
    cancelled_at: Optional[datetime]
    cancellation_reason: Optional[str]
    created_at: datetime
    updated_at: datetime
    version: int

    # Relationships
    customer: Optional[Customer] = None
    items: Optional[list["OrderItemDetails"]] = None
    payments: Optional[list["Payment"]] = None


@dataclass
class OrderItemDetails:
    """Order item from v_order_items_details view."""

    id: int
    order_id: int
    order_number: str
    product_id: int
    product_sku: str
    product_name: str
    quantity: int
    unit_price: Decimal
    subtotal: Decimal
    current_price: Decimal  # Current price (may differ from unit_price)
    created_at: datetime

    # Relationships
    product: Optional[Product] = None


@dataclass
class Payment:
    """Payment from v_payments view."""

    id: int
    order_id: int
    order_number: str
    customer_id: int
    customer_name: str
    amount: Decimal
    payment_method: str  # 'credit_card', 'debit_card', 'paypal', etc.
    transaction_id: Optional[str]
    status: str  # 'pending', 'completed', 'failed', 'refunded'
    processed_at: Optional[datetime]
    refunded_at: Optional[datetime]
    refund_amount: Optional[Decimal]
    notes: Optional[str]
    created_at: datetime


@dataclass
class RevenueByProduct:
    """Revenue analytics from v_revenue_by_product view."""

    product_id: int
    sku: str
    product_name: str
    orders_count: int
    units_sold: int
    total_revenue: Decimal
    average_price: Decimal
    min_price: Decimal
    max_price: Decimal
    current_price: Decimal
    current_cost: Decimal
    estimated_profit: Decimal


@dataclass
class CustomerLifetimeValue:
    """Customer lifetime value from v_customer_lifetime_value view."""

    customer_id: int
    email: str
    customer_name: str
    country: str
    total_orders: int
    completed_orders: int
    cancelled_orders: int
    lifetime_value: Decimal
    average_order_value: Decimal
    first_order_date: Optional[datetime]
    last_order_date: Optional[datetime]
    customer_since: datetime


@dataclass
class AuditLog:
    """Audit log entry from v_audit_log view."""

    id: int
    operation: str  # 'INSERT', 'UPDATE', 'DELETE'
    entity_type: str
    entity_id: int
    changed_by: Optional[str]
    old_values: Optional[dict]
    new_values: Optional[dict]
    changes: Optional[dict]
    ip_address: Optional[str]
    user_agent: Optional[str]
    created_at: datetime
    order_number: Optional[str]
    customer_email: Optional[str]


@dataclass
class OrderStatusTimeline:
    """Order status timeline from v_order_status_timeline view."""

    order_id: int
    order_number: str
    order_created: datetime
    paid_at: Optional[datetime]
    shipped_at: Optional[datetime]
    delivered_at: Optional[datetime]
    cancelled_at: Optional[datetime]
    hours_to_payment: Optional[float]
    hours_to_shipment: Optional[float]
    hours_to_delivery: Optional[float]
    total_fulfillment_hours: Optional[float]
    status: str


# ============================================================================
# INPUT TYPES (Write Model)
# ============================================================================


@dataclass
class OrderItemInput:
    """Input for order items when creating an order."""

    product_id: int
    quantity: int


@dataclass
class CreateOrderInput:
    """Input for creating a new order."""

    customer_id: int
    items: list[OrderItemInput]
    notes: Optional[str] = None


@dataclass
class ProcessPaymentInput:
    """Input for processing payment."""

    order_id: int
    amount: Decimal
    payment_method: str
    transaction_id: Optional[str] = None
    version: Optional[int] = None  # For optimistic locking


@dataclass
class CancelOrderInput:
    """Input for cancelling an order."""

    order_id: int
    reason: str


@dataclass
class UpdateOrderStatusInput:
    """Input for updating order status."""

    order_id: int
    new_status: str


@dataclass
class AddProductInput:
    """Input for adding a new product."""

    sku: str
    name: str
    description: Optional[str]
    price: Decimal
    cost: Decimal
    quantity_available: int = 0


@dataclass
class UpdateProductInventoryInput:
    """Input for updating product inventory."""

    product_id: int
    quantity_change: int  # Positive to add, negative to remove
