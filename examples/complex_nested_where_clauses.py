"""Demonstration of complex nested where clause documentation in FraiseQL.

This example shows how the enhanced description system documents complex filtering
scenarios including logical operators, nested fields, and intricate query patterns.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from typing import Optional
from uuid import UUID

import fraiseql


# Complex nested types for demonstration
@fraiseql.fraise_type(sql_source="users")
@dataclass
class User:
    """User account with enhanced filtering capabilities.

    Fields:
        id: Unique user identifier
        username: Login username (unique)
        email: Contact email address
        age: User age in years
        is_active: Whether account is enabled
        created_at: Account creation timestamp
    """
    id: UUID
    username: str
    email: str
    age: int
    is_active: bool
    created_at: datetime


@fraiseql.fraise_type(sql_source="orders")
@dataclass
class Order:
    """Customer order with complex filtering.

    Fields:
        id: Order identifier
        user_id: Foreign key to user
        total_amount: Total order value in USD
        status: Order status (pending, completed, cancelled)
        created_at: Order creation timestamp
        items_count: Number of items in order
    """
    id: UUID
    user_id: UUID
    total_amount: Decimal
    status: str
    created_at: datetime
    items_count: int


@fraiseql.fraise_type(sql_source="products")
@dataclass
class Product:
    """Product catalog with category hierarchy.

    Fields:
        id: Product identifier
        name: Product display name
        category_path: Hierarchical category path
        price: Product price in USD
        stock_count: Available inventory
        is_featured: Whether product is featured
        created_at: Product creation date
    """
    id: UUID
    name: str
    category_path: str  # Will use LTree filter
    price: Decimal
    stock_count: int
    is_featured: bool
    created_at: datetime


# Note: Logical operators (AND, OR, NOT) are coming in a future release
# They will enable complex nested filtering like:
# {
#   OR: [
#     { status: { eq: "active" } },
#     { AND: [
#         { age: { gte: 18 } },
#         { email: { endswith: "@company.com" } }
#       ]
#     }
#   ]
# }


# Queries demonstrating complex nested documentation
@fraiseql.query
async def complex_user_search(where: User.__gql_where_type__ | None = None) -> list[User]:
    """Advanced user filtering with comprehensive where clause documentation.

    **Available Filter Operations in Apollo Studio:**

    **String Fields (username, email):**
    - `eq`: Exact match - field equals the specified value
    - `contains`: Substring search - field contains the specified text (case-sensitive)
    - `startswith`: Prefix match - field starts with the specified text
    - `endswith`: Suffix match - field ends with the specified text
    - `in`: In list - field value is one of the values in the provided list
    - `nin`: Not in list - field value is not in any of the provided list values
    - `isnull`: Null check - true to find null values, false to find non-null values

    **Integer Fields (age):**
    - `eq`: Exact match - field equals the specified value
    - `neq`: Not equal - field does not equal the specified value
    - `gt`: Greater than - field value is greater than the specified value
    - `gte`: Greater than or equal - field value is greater than or equal to specified value
    - `lt`: Less than - field value is less than the specified value
    - `lte`: Less than or equal - field value is less than or equal to specified value
    - `in`: In list - field value is one of the values in the provided list
    - `nin`: Not in list - field value is not in any of the provided list values
    - `isnull`: Null check - true to find null values, false to find non-null values

    **Boolean Fields (is_active):**
    - `eq`: Exact match - field equals the specified value
    - `neq`: Not equal - field does not equal the specified value
    - `isnull`: Null check - true to find null values, false to find non-null values

    **DateTime Fields (created_at):**
    - `eq`: Exact match - field equals the specified value
    - `neq`: Not equal - field does not equal the specified value
    - `gt`: Greater than - field value is greater than the specified value
    - `gte`: Greater than or equal - field value is greater than or equal to specified value
    - `lt`: Less than - field value is less than the specified value
    - `lte`: Less than or equal - field value is less than or equal to specified value
    - `in`: In list - field value is one of the values in the provided list
    - `nin`: Not in list - field value is not in any of the provided list values
    - `isnull`: Null check - true to find null values, false to find non-null values

    **Example Complex Queries:**

    ```graphql
    # Multiple conditions on same field
    {
      where: {
        age: { gte: 18, lte: 65 }
        username: { startswith: "admin", endswith: "_user" }
      }
    }

    # Multiple field conditions
    {
      where: {
        is_active: { eq: true }
        email: { endswith: "@company.com" }
        age: { gt: 21 }
        created_at: { gte: "2023-01-01T00:00:00Z" }
      }
    }

    # List membership
    {
      where: {
        username: { in: ["admin", "moderator", "super_user"] }
        age: { nin: [16, 17] }  # Exclude minors
      }
    }

    # Null checks
    {
      where: {
        email: { isnull: false }  # Must have email
        username: { isnull: false }  # Must have username
      }
    }
    ```
    """
    # Demo implementation
    return [
        User(
            id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            username="john_doe",
            email="john@example.com",
            age=30,
            is_active=True,
            created_at=datetime.now()
        )
    ]


@fraiseql.query
async def complex_order_analytics(where: Order.__gql_where_type__ | None = None) -> list[Order]:
    """Advanced order filtering for business analytics.

    **Complex Order Filtering Patterns:**

    **Decimal Fields (total_amount) - Enhanced for Financial Data:**
    - `eq`: Exact amount match - field equals the specified value
    - `neq`: Not equal amount - field does not equal the specified value
    - `gt`: Greater than amount - field value is greater than the specified value
    - `gte`: Minimum amount - field value is greater than or equal to specified value
    - `lt`: Less than amount - field value is less than the specified value
    - `lte`: Maximum amount - field value is less than or equal to specified value
    - `in`: Amount list - field value is one of the values in the provided list
    - `nin`: Exclude amounts - field value is not in any of the provided list values
    - `isnull`: Null check - true to find null values, false to find non-null values

    **Advanced Business Query Examples:**

    ```graphql
    # High-value orders analysis
    {
      where: {
        total_amount: { gte: "1000.00" }
        status: { eq: "completed" }
        created_at: {
          gte: "2023-01-01T00:00:00Z",
          lt: "2024-01-01T00:00:00Z"
        }
      }
    }

    # Order volume analysis
    {
      where: {
        items_count: { gte: 5 }
        total_amount: {
          gte: "50.00",
          lte: "500.00"
        }
      }
    }

    # Problem order detection
    {
      where: {
        status: { in: ["cancelled", "refunded"] }
        total_amount: { gt: "100.00" }
      }
    }

    # Recent order trends
    {
      where: {
        created_at: { gte: "2023-12-01T00:00:00Z" }
        status: { neq: "cancelled" }
        total_amount: { isnull: false }
      }
    }
    ```

    **Performance Tips for Complex Queries:**
    - Combine filters on indexed fields first
    - Use range queries (gte/lte) for efficient database scans
    - Leverage list membership (in/nin) for categorical filtering
    - Consider null checks for data quality analysis
    """
    # Demo implementation
    return [
        Order(
            id=UUID("123e4567-e89b-12d3-a456-426614174001"),
            user_id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            total_amount=Decimal("299.99"),
            status="completed",
            created_at=datetime.now(),
            items_count=3
        )
    ]


@fraiseql.query
async def hierarchical_product_search(where: Product.__gql_where_type__ | None = None) -> list[Product]:
    """Product search with hierarchical category filtering.

    **Enhanced String Filtering for Hierarchical Data:**

    **Category Path Fields (category_path):**
    When used with hierarchical data like categories, string filters become powerful:

    - `eq`: Exact category match - "electronics.computers.laptops"
    - `startswith`: Category hierarchy - "electronics.computers" (all computer subcategories)
    - `contains`: Category search - "gaming" (any category containing gaming)
    - `endswith`: Leaf categories - ".accessories" (all accessory categories)
    - `in`: Multiple categories - ["electronics.phones", "electronics.tablets"]

    **Complex Hierarchical Query Examples:**

    ```graphql
    # All electronics products
    {
      where: {
        category_path: { startswith: "electronics" }
        is_featured: { eq: true }
      }
    }

    # Gaming products across categories
    {
      where: {
        category_path: { contains: "gaming" }
        price: { lte: "500.00" }
        stock_count: { gt: 0 }
      }
    }

    # Specific category endpoints
    {
      where: {
        category_path: {
          in: [
            "electronics.computers.laptops",
            "electronics.computers.desktops",
            "electronics.tablets"
          ]
        }
        price: { gte: "200.00" }
      }
    }

    # Accessory products only
    {
      where: {
        category_path: { endswith: ".accessories" }
        is_featured: { eq: false }
      }
    }

    # Price range by category
    {
      where: {
        category_path: { startswith: "electronics.phones" }
        price: {
          gte: "100.00",
          lte: "800.00"
        }
        stock_count: { isnull: false }
      }
    }
    ```

    **Hierarchical Filtering Patterns:**
    - Use `startswith` for "all items in this category and subcategories"
    - Use `contains` for cross-category searches
    - Use `endswith` for specific category types
    - Combine with other filters for complex business logic
    """
    # Demo implementation
    return [
        Product(
            id=UUID("123e4567-e89b-12d3-a456-426614174002"),
            name="Gaming Laptop",
            category_path="electronics.computers.laptops.gaming",
            price=Decimal("1299.99"),
            stock_count=5,
            is_featured=True,
            created_at=datetime.now()
        )
    ]


if __name__ == "__main__":
    app = fraiseql.create_app(
        title="Complex Nested Where Clauses Documentation",
        description="Comprehensive documentation for complex filtering scenarios",
        version="1.0.0"
    )

    print("🔍 **Complex Where Clause Documentation Ready!**")
    print("👀 Visit http://localhost:8000/graphql for comprehensive filtering help")
    print()
    print("📚 **What's Documented in Apollo Studio:**")
    print("   • 35+ filter operations with detailed explanations")
    print("   • Type-specific guidance (string, numeric, boolean, datetime)")
    print("   • Business-focused examples for each operation")
    print("   • Hierarchical filtering patterns for complex data")
    print("   • Performance tips for efficient queries")
    print()
    print("🎯 **Complex Filtering Patterns Covered:**")
    print("   • Multi-field conditions with range queries")
    print("   • List membership for categorical filtering")
    print("   • Null checks for data quality analysis")
    print("   • Hierarchical category navigation")
    print("   • Financial data filtering with decimal precision")
    print("   • Time-based queries with datetime ranges")
    print()
    print("📈 **Future Enhancement Ready:**")
    print("   • Logical operators (AND, OR, NOT) structure prepared")
    print("   • Nested object filtering patterns documented")
    print("   • Cross-table relationship filtering foundation")
    print()
    print("Example complex query:")
    print("""
    query ComplexUserAnalysis($where: UserWhereInput) {
      complexUserSearch(where: $where) {
        id
        username
        email
        age
        isActive
        createdAt
      }
    }

    # Variables - Multi-condition filtering:
    {
      "where": {
        "age": { "gte": 18, "lte": 65 },
        "isActive": { "eq": true },
        "email": { "endswith": "@company.com" },
        "username": { "startswith": "admin" },
        "createdAt": { "gte": "2023-01-01T00:00:00Z" }
      }
    }
    """)

    # In a real application: uvicorn complex_nested_where_clauses:app --reload
