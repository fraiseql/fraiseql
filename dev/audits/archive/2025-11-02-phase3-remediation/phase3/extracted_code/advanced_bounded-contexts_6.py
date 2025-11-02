# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 6
# shared/types.py
from dataclasses import dataclass
from decimal import Decimal


@dataclass
class Money:
    """Shared money type."""

    amount: Decimal
    currency: str = "USD"

    def __add__(self, other: "Money") -> "Money":
        if self.currency != other.currency:
            raise ValueError("Cannot add different currencies")
        return Money(self.amount + other.amount, self.currency)

    def __mul__(self, scalar: float) -> "Money":
        return Money(self.amount * Decimal(str(scalar)), self.currency)


@dataclass
class Address:
    """Shared address type."""

    street: str
    city: str
    state: str
    postal_code: str
    country: str


@dataclass
class CustomerId:
    """Shared customer identifier."""

    value: str

    def __str__(self) -> str:
        return self.value


# Usage in Orders Context
@dataclass
class Order:
    id: UUID
    customer_id: CustomerId  # Shared type
    shipping_address: Address  # Shared type
    items: list["OrderItem"]
    total: Money  # Shared type
    status: str


# Usage in Billing Context
@dataclass
class Invoice:
    id: UUID
    customer_id: CustomerId  # Same shared type
    billing_address: Address  # Same shared type
    amount: Money  # Same shared type
    status: str
