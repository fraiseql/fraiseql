# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 3
from dataclasses import dataclass, field
from datetime import datetime
from decimal import Decimal
from uuid import uuid4


@dataclass
class Order:
    """Order aggregate root - enforces all business rules."""

    id: UUID = field(default_factory=lambda: str(uuid4()))
    customer_id: str = ""
    items: list["OrderItem"] = field(default_factory=list)
    status: str = "draft"
    created_at: datetime = field(default_factory=datetime.utcnow)
    updated_at: datetime = field(default_factory=datetime.utcnow)

    @property
    def total(self) -> Decimal:
        """Calculate total from items."""
        return sum(item.total for item in self.items)

    def add_item(self, product_id: str, quantity: int, price: Decimal):
        """Add item to order - enforces business rules."""
        if self.status != "draft":
            raise ValueError("Cannot modify non-draft order")

        if quantity <= 0:
            raise ValueError("Quantity must be positive")

        # Check if product already in order
        for item in self.items:
            if item.product_id == product_id:
                item.quantity += quantity
                item.total = item.price * item.quantity
                self.updated_at = datetime.utcnow()
                return

        # Add new item
        item = OrderItem(
            id=str(uuid4()),
            order_id=self.id,
            product_id=product_id,
            quantity=quantity,
            price=price,
            total=price * quantity,
        )
        self.items.append(item)
        self.updated_at = datetime.utcnow()

    def remove_item(self, product_id: str):
        """Remove item from order."""
        if self.status != "draft":
            raise ValueError("Cannot modify non-draft order")

        self.items = [item for item in self.items if item.product_id != product_id]
        self.updated_at = datetime.utcnow()

    def submit(self):
        """Submit order for processing - state transition."""
        if self.status != "draft":
            raise ValueError("Order already submitted")

        if not self.items:
            raise ValueError("Cannot submit empty order")

        if not self.customer_id:
            raise ValueError("Customer ID required")

        self.status = "submitted"
        self.updated_at = datetime.utcnow()

    def cancel(self):
        """Cancel order."""
        if self.status in ["shipped", "delivered"]:
            raise ValueError(f"Cannot cancel {self.status} order")

        self.status = "cancelled"
        self.updated_at = datetime.utcnow()


@dataclass
class OrderItem:
    """Order item - part of Order aggregate."""

    id: UUID
    order_id: str
    product_id: str
    quantity: int
    price: Decimal
    total: Decimal
