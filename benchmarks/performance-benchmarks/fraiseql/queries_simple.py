"""FraiseQL benchmark queries - simplified version."""

from typing import List

from models import Category, Order, PopularProduct, Product, ProductsByCategory, User, UserStats

from fraiseql import fraise_type


@fraise_type
class Query:
    """Root query type for the benchmark API."""

    # Try without field() to see if it works
    users: List[User]
    products: List[Product]
    orders: List[Order]
    categories: List[Category]
    popular_products: List[PopularProduct]
    products_by_category: List[ProductsByCategory]
    user_stats: List[UserStats]
