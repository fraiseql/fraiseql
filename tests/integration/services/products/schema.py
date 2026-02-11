"""Products subgraph schema - extends Order, owns Product entity"""

from fraiseql import type, key, extends, external, ID
from typing import Optional


@extends
@key(fields=["id"])
@type
class Order:
    """Order extended from orders subgraph"""

    id: ID = external()
    products: list["Product"]


@type
@key(fields=["id"])
class Product:
    """Product entity - owned by this subgraph"""

    id: ID
    identifier: str
    name: str
    description: Optional[str]
    price: float
    stock: int


@type
class Query:
    """Root query type"""

    def product(self, id: ID) -> Optional[Product]:
        """Get product by ID"""
        pass

    def products(self) -> list[Product]:
        """Get all products"""
        pass

    def products_in_stock(self) -> list[Product]:
        """Get products in stock"""
        pass

    def products_by_price_range(self, min_price: float, max_price: float) -> list[Product]:
        """Get products in price range"""
        pass


@type
class Mutation:
    """Root mutation type"""

    def create_product(
        self,
        identifier: str,
        name: str,
        price: float,
        stock: int,
        description: Optional[str] = None,
    ) -> Product:
        """Create a new product"""
        pass

    def update_product_stock(self, id: ID, stock: int) -> Optional[Product]:
        """Update product stock"""
        pass

    def update_product_price(self, id: ID, price: float) -> Optional[Product]:
        """Update product price"""
        pass

    def delete_product(self, id: ID) -> bool:
        """Delete product"""
        pass
