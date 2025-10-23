"""Demonstration of EXISTING complex nested where clause syntax in FraiseQL.

This example shows that FraiseQL ALREADY SUPPORTS complex logical operators (AND, OR, NOT)
and how the enhanced description system now makes them self-documenting in Apollo Studio!
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from uuid import UUID

import fraiseql
from fraiseql.sql import create_graphql_where_input


# Complex data model for demonstration
@fraiseql.fraise_type(sql_source="users")
@dataclass
class User:
    """User account with comprehensive filtering.

    Fields:
        id: Unique user identifier
        username: Login username (unique)
        email: Contact email address
        age: User age in years
        department: Department name
        salary: Annual salary in USD
        is_active: Whether account is enabled
        is_admin: Whether user has admin privileges
        created_at: Account creation timestamp
        last_login: Last login timestamp
    """
    id: UUID
    username: str
    email: str
    age: int
    department: str
    salary: Decimal
    is_active: bool
    is_admin: bool
    created_at: datetime
    last_login: datetime | None = None


@fraiseql.fraise_type(sql_source="orders")
@dataclass
class Order:
    """Order with complex business filtering.

    Fields:
        id: Order identifier
        user_id: Customer who placed the order
        total_amount: Total order value in USD
        status: Order status (pending, processing, shipped, delivered, cancelled)
        priority: Order priority (low, normal, high, urgent)
        items_count: Number of items in the order
        created_at: Order placement timestamp
        shipped_at: Order shipment timestamp
        delivered_at: Order delivery timestamp
    """
    id: UUID
    user_id: UUID
    total_amount: Decimal
    status: str
    priority: str
    items_count: int
    created_at: datetime
    shipped_at: datetime | None = None
    delivered_at: datetime | None = None


# Create advanced where input types using the existing FraiseQL functionality
UserWhereInput = create_graphql_where_input(User)
OrderWhereInput = create_graphql_where_input(Order)


@fraiseql.query
async def advanced_user_search(where: UserWhereInput | None = None) -> list[User]:
    """Advanced user search with EXISTING complex logical operators.

    **🎉 ALREADY SUPPORTED Complex Where Clause Syntax:**

    **Logical Operators (FULLY IMPLEMENTED):**
    - `AND`: All conditions in the list must be true
    - `OR`: At least one condition in the list must be true
    - `NOT`: Negates the given condition

    **Real Examples That Work RIGHT NOW:**

    ```graphql
    # Complex OR conditions
    {
      "where": {
        "OR": [
          { "department": { "eq": "Engineering" } },
          { "department": { "eq": "Product" } },
          { "is_admin": { "eq": true } }
        ]
      }
    }

    # Nested AND with OR
    {
      "where": {
        "AND": [
          { "is_active": { "eq": true } },
          {
            "OR": [
              { "age": { "gte": 25, "lte": 45 } },
              { "salary": { "gte": "80000" } }
            ]
          }
        ]
      }
    }

    # NOT operator for exclusions
    {
      "where": {
        "NOT": {
          "department": { "in": ["Intern", "Contractor"] }
        }
      }
    }

    # Ultra-complex business logic
    {
      "where": {
        "AND": [
          { "is_active": { "eq": true } },
          {
            "OR": [
              {
                "AND": [
                  { "department": { "eq": "Sales" } },
                  { "salary": { "gte": "50000" } }
                ]
              },
              {
                "AND": [
                  { "department": { "eq": "Engineering" } },
                  { "age": { "lte": 35 } }
                ]
              },
              { "is_admin": { "eq": true } }
            ]
          },
          {
            "NOT": {
              "email": { "endswith": "@contractor.com" }
            }
          }
        ]
      }
    }
    ```

    **All filter operations have enhanced descriptions in Apollo Studio:**
    - String operations: eq, contains, startswith, endswith, in, nin, isnull
    - Numeric operations: eq, neq, gt, gte, lt, lte, in, nin, isnull
    - Boolean operations: eq, neq, isnull
    - DateTime operations: Full range and equality comparisons
    - Logical operations: AND, OR, NOT with full nesting support
    """
    # Demo implementation - in real app this would execute the complex query
    return [
        User(
            id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            username="john_doe",
            email="john@company.com",
            age=30,
            department="Engineering",
            salary=Decimal("95000.00"),
            is_active=True,
            is_admin=False,
            created_at=datetime.now(),
            last_login=datetime.now()
        )
    ]


@fraiseql.query
async def complex_order_analysis(where: OrderWhereInput | None = None) -> list[Order]:
    """Complex order analysis with advanced logical filtering.

    **Business Intelligence Queries Made Easy:**

    ```graphql
    # High-value problem orders
    {
      "where": {
        "AND": [
          { "total_amount": { "gte": "500.00" } },
          {
            "OR": [
              { "status": { "eq": "cancelled" } },
              {
                "AND": [
                  { "status": { "eq": "shipped" } },
                  { "shipped_at": { "lt": "2023-11-01T00:00:00Z" } },
                  { "delivered_at": { "isnull": true } }
                ]
              }
            ]
          }
        ]
      }
    }

    # Urgent order backlog analysis
    {
      "where": {
        "AND": [
          { "priority": { "in": ["high", "urgent"] } },
          { "status": { "in": ["pending", "processing"] } },
          {
            "OR": [
              { "created_at": { "lt": "2023-12-01T00:00:00Z" } },
              { "items_count": { "gte": 10 } }
            ]
          },
          {
            "NOT": {
              "total_amount": { "lt": "50.00" }
            }
          }
        ]
      }
    }

    # Successful delivery patterns
    {
      "where": {
        "AND": [
          { "status": { "eq": "delivered" } },
          { "delivered_at": { "isnull": false } },
          {
            "OR": [
              {
                "AND": [
                  { "priority": { "eq": "normal" } },
                  { "total_amount": { "gte": "100.00", "lte": "500.00" } }
                ]
              },
              {
                "AND": [
                  { "priority": { "eq": "high" } },
                  { "items_count": { "lte": 5 } }
                ]
              }
            ]
          }
        ]
      }
    }
    ```

    **Performance Optimized:**
    - Logical operators generate efficient SQL with proper parentheses
    - Database indexes can be utilized effectively
    - Complex business logic maps directly to PostgreSQL queries
    """
    # Demo implementation
    return [
        Order(
            id=UUID("123e4567-e89b-12d3-a456-426614174001"),
            user_id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            total_amount=Decimal("749.99"),
            status="delivered",
            priority="normal",
            items_count=3,
            created_at=datetime.now(),
            shipped_at=datetime.now(),
            delivered_at=datetime.now()
        )
    ]


@fraiseql.query
async def cross_entity_insights(
    user_where: UserWhereInput | None = None,
    order_where: OrderWhereInput | None = None
) -> dict:
    """Demonstrate independent complex filtering on multiple entities.

    **Multi-Entity Business Intelligence:**

    ```graphql
    query CrossEntityAnalysis(
      $userWhere: UserWhereInput,
      $orderWhere: OrderWhereInput
    ) {
      insights: crossEntityInsights(
        userWhere: $userWhere,
        orderWhere: $orderWhere
      ) {
        summary
        user_count
        order_count
        total_revenue
      }
    }

    # Variables - Complex multi-entity filtering:
    {
      "userWhere": {
        "AND": [
          { "is_active": { "eq": true } },
          {
            "OR": [
              { "department": { "eq": "Sales" } },
              { "is_admin": { "eq": true } }
            ]
          }
        ]
      },
      "orderWhere": {
        "AND": [
          { "status": { "in": ["delivered", "shipped"] } },
          { "total_amount": { "gte": "200.00" } },
          {
            "NOT": {
              "priority": { "eq": "low" }
            }
          }
        ]
      }
    }
    ```
    """
    # Demo analysis result
    return {
        "summary": "Complex multi-entity analysis",
        "user_count": 1,
        "order_count": 1,
        "total_revenue": "749.99"
    }


if __name__ == "__main__":
    app = fraiseql.create_app(
        title="EXISTING Complex Where Clause Syntax",
        description="FraiseQL ALREADY supports complex nested logical operators!",
        version="1.0.0"
    )

    print("🚀 **FraiseQL Already Has Complex Where Clause Support!**")
    print("👀 Visit http://localhost:8000/graphql to see it in action")
    print()
    print("✅ **ALREADY IMPLEMENTED Features:**")
    print("   • Logical operators: AND, OR, NOT")
    print("   • Unlimited nesting depth")
    print("   • Field + logical operator combinations")
    print("   • Efficient SQL generation with parentheses")
    print("   • Full type safety with GraphQL schema")
    print()
    print("🎯 **NEW Enhancement (Just Added):**")
    print("   • Automatic descriptions for ALL operators")
    print("   • Apollo Studio tooltips for logical operations")
    print("   • Self-documenting complex query syntax")
    print("   • Enhanced developer experience")
    print()
    print("📚 **What You Get in Apollo Studio NOW:**")
    print("   • AND: 'Logical AND - all conditions in the list must be true'")
    print("   • OR: 'Logical OR - at least one condition in the list must be true'")
    print("   • NOT: 'Logical NOT - negates the given condition'")
    print("   • Plus all 35+ field operations with detailed explanations")
    print()
    print("🎉 **The syntax was already there - now it's beautifully documented!**")

    # In a real application: uvicorn existing_complex_where_syntax:app --reload
