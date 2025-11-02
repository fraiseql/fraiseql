# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 2
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from uuid import UUID


# Orders Context Domain Model
@dataclass
class Order:
    """Order aggregate root."""

    id: UUID
    customer_id: UUID
    items: list["OrderItem"]
    total: Decimal
    status: str
    created_at: datetime
    updated_at: datetime


@dataclass
class OrderItem:
    """Order line item."""

    id: UUID
    order_id: UUID
    product_id: UUID
    quantity: int
    price: Decimal
    total: Decimal
