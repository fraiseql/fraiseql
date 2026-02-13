"""
Multi-Cloud Orders Service (GCP eu-west)
Owns Order entity, extends User
"""

from fraiseql import type, key, extends, external, requires
from typing import Optional


@type
@extends
@key(fields=["id"])
class User:
    """User entity extended from users-service"""
    id: str = external()
    orders: list["Order"] = requires(fields=["id"])


@type
@key(fields=["id"])
class Order:
    """Order entity owned by orders-service"""
    id: str
    user_id: str
    status: str
    total: float
    created_at: str


@type
class Query:
    """Root query type"""

    def order(self, id: str) -> Optional[Order]:
        """Get order by ID"""
        pass

    def orders(self) -> list[Order]:
        """Get all orders"""
        pass

    def orders_by_status(self, status: str) -> list[Order]:
        """Get orders by status"""
        pass

    def user_orders(self, user_id: str) -> list[Order]:
        """Get orders for user"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_order(self, id: str, user_id: str, total: float) -> Order:
        """Create order"""
        pass

    def update_order_status(self, id: str, status: str) -> Optional[Order]:
        """Update order status"""
        pass

    def cancel_order(self, id: str) -> bool:
        """Cancel order"""
        pass
