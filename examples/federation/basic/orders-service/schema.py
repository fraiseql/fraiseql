"""
Orders Service Schema
Owns the Order entity and extends User
"""

from fraiseql import type, key, extends, external
from typing import Optional


@type
@extends
@key("id")
class User:
    """
    User entity (extended from users-service)
    Only certain fields are resolved locally
    """
    id: str = external()
    email: str = external()
    orders: list["Order"]


@type
@key("id")
class Order:
    """
    Order entity
    Owned by orders-service
    References User from users-service
    """
    id: str
    user_id: str
    status: str
    total: float


@type
class Query:
    """Root query type"""

    def order(self, id: str) -> Optional[Order]:
        """Get a single order by ID"""
        # FraiseQL automatically resolves from database
        pass

    def orders(self) -> list[Order]:
        """Get all orders"""
        # FraiseQL automatically resolves from database
        pass

    def user_orders(self, user_id: str) -> list[Order]:
        """Get orders for a specific user"""
        # FraiseQL automatically resolves with WHERE user_id = ?
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_order(self, user_id: str, total: float) -> Order:
        """Create a new order"""
        # FraiseQL automatically handles INSERT
        pass

    def update_order_status(self, id: str, status: str) -> Optional[Order]:
        """Update order status"""
        # FraiseQL automatically handles UPDATE
        pass

    def cancel_order(self, id: str) -> bool:
        """Cancel an order"""
        # FraiseQL automatically handles UPDATE with status='cancelled'
        pass
