"""FraiseQL benchmark queries - simplified version."""

from models import Category, Order, PopularProduct, Product, ProductsByCategory, User, UserStats

from fraiseql import fraise_type


@fraise_type
class Query:
    """Root query type for the benchmark API."""

    # Try without field() to see if it works
    users: list[User]
    products: list[Product]
    orders: list[Order]
    categories: list[Category]
    popular_products: list[PopularProduct]
    products_by_category: list[ProductsByCategory]
    user_stats: list[UserStats]
