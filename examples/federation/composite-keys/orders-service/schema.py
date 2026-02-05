"""
Multi-tenant Orders Service Schema
Extends User with composite key federation
"""

from fraiseql import type, key, extends, external, requires
from typing import Optional


@type
@extends
@key(fields=["organization_id", "user_id"])
class User:
    """
    User entity extended from users-service
    Composite key: (organizationId, userId)
    """
    organization_id: str = external()
    user_id: str = external()
    orders: list["Order"] = requires(fields=["organization_id", "user_id"])


@type
@key(fields=["organization_id", "order_id"])
class Order:
    """
    Order entity with composite key
    Identifies orders uniquely within organization
    """
    organization_id: str
    order_id: str
    user_id: str
    status: str
    amount: float


@type
class Query:
    """Root query type"""

    def order(
        self,
        organization_id: str,
        order_id: str,
    ) -> Optional[Order]:
        """Get single order (composite key)"""
        pass

    def orders(self, organization_id: str) -> list[Order]:
        """Get all orders in organization"""
        pass

    def user_orders(
        self,
        organization_id: str,
        user_id: str,
    ) -> list[Order]:
        """Get orders for specific user in organization"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_order(
        self,
        organization_id: str,
        user_id: str,
        amount: float,
    ) -> Order:
        """Create order for user in organization"""
        pass

    def update_order_status(
        self,
        organization_id: str,
        order_id: str,
        status: str,
    ) -> Optional[Order]:
        """Update order status (composite key)"""
        pass

    def cancel_order(
        self,
        organization_id: str,
        order_id: str,
    ) -> bool:
        """Cancel order in organization"""
        pass
