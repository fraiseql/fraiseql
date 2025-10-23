"""Demonstration of enhanced where clause descriptions in FraiseQL.

This example showcases how filter types now automatically generate comprehensive
field descriptions that appear in Apollo Studio, making where clauses much more
developer-friendly.
"""

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from uuid import UUID

import fraiseql


@fraiseql.fraise_type(sql_source="users")
@dataclass
class User:
    """User account with enhanced filtering capabilities.

    Fields:
        id: Unique user identifier
        username: Login username (unique)
        email: Contact email address
        full_name: Complete display name
        age: User age in years
        salary: Annual salary in USD
        is_active: Whether account is enabled
        created_at: Account creation timestamp
    """
    id: UUID
    username: str
    email: str
    full_name: str
    age: int
    salary: Decimal
    is_active: bool
    created_at: datetime


@fraiseql.fraise_type(sql_source="products")
@dataclass
class Product:
    """Product catalog with rich filtering options.

    Fields:
        id: Product identifier
        name: Product display name
        description: Product description
        price: Price in USD
        stock_count: Available inventory
        category: Product category
        created_at: Product creation date
    """
    id: UUID
    name: str
    description: str
    price: Decimal
    stock_count: int
    category: str
    created_at: datetime


# Example queries demonstrating enhanced where clause documentation
@fraiseql.query
async def search_users(where: User.__gql_where_type__ | None = None) -> list[User]:
    """Search users with comprehensive filtering options.

    The where parameter now has enhanced descriptions for all filter operations:

    - String fields (username, email, full_name):
      * eq: Exact match
      * contains: Substring search
      * startswith: Prefix match
      * in: Value in list
      * isnull: Null check

    - Numeric fields (age, salary):
      * eq, neq: Equality checks
      * gt, gte, lt, lte: Range comparisons
      * in, nin: List membership

    - Boolean fields (is_active):
      * eq, neq: Boolean comparison
      * isnull: Null check

    - DateTime fields (created_at):
      * eq, neq: Exact timestamp match
      * gt, gte, lt, lte: Time range queries
      * in, nin: Timestamp list
    """
    # In a real app, this would use the where clause to filter database results
    # For demo purposes, return sample data
    return [
        User(
            id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            username="john_doe",
            email="john@example.com",
            full_name="John Doe",
            age=30,
            salary=Decimal("75000.00"),
            is_active=True,
            created_at=datetime.now()
        )
    ]


@fraiseql.query
async def filter_products(where: Product.__gql_where_type__ | None = None) -> list[Product]:
    """Filter products with enhanced where clause help.

    All filter operations now have helpful descriptions in Apollo Studio:

    Example queries you can build:
    - Find products by name: { name: { contains: "laptop" } }
    - Price range: { price: { gte: 100, lte: 500 } }
    - In stock: { stock_count: { gt: 0 } }
    - Multiple categories: { category: { in: ["electronics", "computers"] } }
    - Recent products: { created_at: { gte: "2023-01-01T00:00:00Z" } }
    """
    # Demo data
    return [
        Product(
            id=UUID("123e4567-e89b-12d3-a456-426614174001"),
            name="Gaming Laptop",
            description="High-performance gaming laptop",
            price=Decimal("1299.99"),
            stock_count=5,
            category="electronics",
            created_at=datetime.now()
        )
    ]


# Custom filter types also get automatic descriptions
@fraiseql.fraise_input
@dataclass
class AdvancedUserFilter:
    """Advanced user filtering with custom operations.

    This custom filter demonstrates how any class ending in 'Filter'
    automatically gets comprehensive field descriptions.
    """
    # Standard filter operations get automatic descriptions
    username: str | None = None  # Gets: "username operation"
    email_verified: bool | None = None  # Gets: "email_verified operation"

    # You can still provide explicit descriptions
    special_status: str | None = fraiseql.fraise_field(
        description="Custom status filter with business logic"
    )


@fraiseql.query
async def advanced_user_search(filter: AdvancedUserFilter | None = None) -> list[User]:
    """Advanced user search with custom filter descriptions."""
    return await search_users()


if __name__ == "__main__":
    # Create the FastAPI application
    app = fraiseql.create_app(
        title="Enhanced Where Clauses Example",
        description="Demonstration of automatic where clause descriptions",
        version="1.0.0"
    )

    print("🚀 GraphQL API with enhanced where clause descriptions is ready!")
    print("👀 Visit http://localhost:8000/graphql to see the enhanced documentation")
    print()
    print("🔍 **Where Clause Help Now Available:**")
    print("   • All filter operations have detailed descriptions")
    print("   • String operations: eq, contains, startswith, etc.")
    print("   • Numeric operations: gt, gte, lt, lte, ranges")
    print("   • Boolean operations: eq, neq, isnull")
    print("   • DateTime operations: timestamp comparisons")
    print("   • Network operations: IP/CIDR specific filters")
    print()
    print("📚 **Apollo Studio Integration:**")
    print("   • Hover over filter fields to see operation descriptions")
    print("   • Query builder shows helpful tooltips")
    print("   • Schema explorer documents all filter types")
    print("   • IntelliSense works with comprehensive field docs")
    print()
    print("Example GraphQL query:")
    print("""
    query SearchUsers($where: UserWhereInput) {
      searchUsers(where: $where) {
        id
        username
        email
        fullName
        age
        isActive
      }
    }

    # Variables:
    {
      "where": {
        "age": { "gte": 18, "lte": 65 },
        "isActive": { "eq": true },
        "username": { "contains": "john" }
      }
    }
    """)

    # In a real application, you would run: uvicorn enhanced_where_clauses:app --reload
