"""Example: Analytics schema with fact tables and aggregate queries.

This example demonstrates how to define fact tables for analytics workloads.
"""

import fraiseql


# Define a fact table type
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity", "cost"],
    dimension_paths=[
        {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
        {"name": "product_name", "json_path": "data->>'product_name'", "data_type": "text"},
        {"name": "region", "json_path": "data->>'region'", "data_type": "text"},
    ],
)
@fraiseql.type
class Sale:
    """Sales transaction fact table.

    Fact table pattern:
    - Measures: revenue, quantity, cost (numeric columns for aggregation)
    - Dimensions: category, product_name, region (JSONB data column)
    - Denormalized filters: customer_id, product_id, occurred_at (indexed columns)
    """

    id: int
    # Measures (aggregatable)
    revenue: float
    quantity: int
    cost: float
    # Denormalized filters (indexed for fast WHERE)
    customer_id: str
    product_id: str
    occurred_at: str


# Define an aggregate query
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True,
)
@fraiseql.query
def sales_aggregate() -> list[dict]:
    """Aggregate sales data with flexible grouping.

    Supports:
    - GROUP BY: category, product_name, region, occurred_at (day/week/month/year)
    - Aggregates: count, revenue_sum, revenue_avg, quantity_sum, cost_sum
    - WHERE: customer_id, product_id, occurred_at range
    - HAVING: revenue_sum_gt, quantity_sum_gte, etc.
    - ORDER BY: any aggregate or dimension
    - LIMIT/OFFSET: pagination

    Example GraphQL query:
    ```graphql
    query {
      sales_aggregate(
        where: {
          occurred_at: { _gte: "2025-01-01", _lt: "2025-02-01" }
          customer_id: { _eq: "customer-123" }
        }
        groupBy: {
          category: true
          occurred_at_day: true
        }
        having: {
          revenue_sum_gt: 1000
        }
        orderBy: [
          { field: "revenue_sum", direction: DESC }
        ]
        limit: 100
      ) {
        category
        occurred_at_day
        count
        revenue_sum
        revenue_avg
        quantity_sum
      }
    }
    ```
    """


# Regular types and queries can coexist
@fraiseql.type
class User:
    """User type."""

    id: int
    name: str
    email: str


@fraiseql.query(sql_source="v_user")
def users(limit: int = 10) -> list[User]:
    """Get all users."""


# Export schema
if __name__ == "__main__":
    fraiseql.export_schema("analytics_schema.json")
    print("✅ Analytics schema exported successfully!")
    print("   Fact tables: 1")
    print("   Aggregate queries: 1")
    print("   Regular types: 1")
    print("   Regular queries: 1")
