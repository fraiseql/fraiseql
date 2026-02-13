"""
Multi-Cloud Products Service (Azure southeast-asia)
Owns Product entity, extends Order
"""

from fraiseql import type, key, extends, external, requires
from typing import Optional


@type
@extends
@key(fields=["id"])
class Order:
    """Order entity extended from orders-service"""
    id: str = external()
    products: list["Product"] = requires(fields=["id"])


@type
@key(fields=["id"])
class Product:
    """Product entity owned by products-service"""
    id: str
    name: str
    price: float
    stock: int
    created_at: str


@type
class Query:
    """Root query type"""

    def product(self, id: str) -> Optional[Product]:
        """Get product by ID"""
        pass

    def products(self) -> list[Product]:
        """Get all products"""
        pass

    def products_by_name(self, name: str) -> list[Product]:
        """Get products by name (partial match)"""
        pass

    def in_stock_products(self) -> list[Product]:
        """Get products in stock"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_product(
        self, id: str, name: str, price: float, stock: int
    ) -> Product:
        """Create product"""
        pass

    def update_product(
        self,
        id: str,
        name: Optional[str] = None,
        price: Optional[float] = None,
        stock: Optional[int] = None,
    ) -> Optional[Product]:
        """Update product"""
        pass

    def delete_product(self, id: str) -> bool:
        """Delete product"""
        pass
