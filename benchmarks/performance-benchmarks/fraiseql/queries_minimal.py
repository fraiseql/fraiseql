"""Minimal FraiseQL Query type to test the setup."""

from models import Product, User

import fraiseql
from fraiseql import fraise_field


@fraiseql.type
class Query:
    """Root query type for the benchmark API."""

    # Add a dummy field to ensure Query has at least one field
    ping: str = fraise_field(default="pong", description="Health check")

    # Database query fields
    users: list[User] = fraise_field(
        default_factory=list, description="Query users with automatic filters"
    )

    products: list[Product] = fraise_field(
        default_factory=list, description="Query products with automatic filters"
    )
