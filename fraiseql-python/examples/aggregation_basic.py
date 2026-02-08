"""Basic aggregation examples using FraiseQL aggregate helpers.

This example demonstrates:
1. Simple table-wide aggregates
2. Aggregates with WHERE clause filtering
3. DISTINCT aggregates
4. Mixed structured and raw SQL aggregates

Requirements:
- FraiseQL 1.9.0+
- PostgreSQL with a `v_orders` view containing JSONB data column
"""

import asyncio
from fraiseql.sql.aggregate_helpers import build_aggregate_dict


async def example_basic_aggregates(db):
    """Example 1: Basic table-wide aggregates."""
    print("Example 1: Basic Aggregates")
    print("=" * 50)

    # Build type-safe aggregates with automatic JSONB casting
    aggregates = build_aggregate_dict({
        "order_count": {"function": "COUNT"},
        "total_revenue": {"function": "SUM", "field": "amount"},
        "avg_order_value": {"function": "AVG", "field": "amount"},
        "min_order": {"function": "MIN", "field": "amount"},
        "max_order": {"function": "MAX", "field": "amount"},
    }, is_jsonb=True)

    # Execute aggregation
    result = await db.aggregate("v_orders", aggregations=aggregates)

    print(f"Order Count: {result['order_count']}")
    print(f"Total Revenue: ${result['total_revenue']:,.2f}")
    print(f"Average Order: ${result['avg_order_value']:,.2f}")
    print(f"Min Order: ${result['min_order']:,.2f}")
    print(f"Max Order: ${result['max_order']:,.2f}")
    print()


async def example_filtered_aggregates(db):
    """Example 2: Aggregates with WHERE clause."""
    print("Example 2: Filtered Aggregates")
    print("=" * 50)

    # Aggregate only completed orders
    aggregates = build_aggregate_dict({
        "completed_count": {"function": "COUNT"},
        "completed_revenue": {"function": "SUM", "field": "amount"},
    }, is_jsonb=True)

    result = await db.aggregate(
        "v_orders",
        aggregations=aggregates,
        where={"status": {"eq": "completed"}}
    )

    print(f"Completed Orders: {result['completed_count']}")
    print(f"Completed Revenue: ${result['completed_revenue']:,.2f}")
    print()


async def example_distinct_aggregates(db):
    """Example 3: DISTINCT aggregates."""
    print("Example 3: Distinct Aggregates")
    print("=" * 50)

    # Count total orders and unique customers
    aggregates = build_aggregate_dict({
        "total_orders": {"function": "COUNT"},
        "unique_customers": {
            "function": "COUNT",
            "field": "customer_id",
            "distinct": True
        },
        "unique_products": {
            "function": "COUNT",
            "field": "product_id",
            "distinct": True
        },
    }, is_jsonb=True)

    result = await db.aggregate("v_orders", aggregations=aggregates)

    print(f"Total Orders: {result['total_orders']}")
    print(f"Unique Customers: {result['unique_customers']}")
    print(f"Unique Products: {result['unique_products']}")
    print(f"Avg Orders per Customer: {result['total_orders'] / result['unique_customers']:.2f}")
    print()


async def example_mixed_aggregates(db):
    """Example 4: Mix structured and raw SQL aggregates."""
    print("Example 4: Mixed Aggregates")
    print("=" * 50)

    aggregates = {
        # Use helpers for simple cases
        **build_aggregate_dict({
            "count": {"function": "COUNT"},
            "sum_amount": {"function": "SUM", "field": "amount"},
            "avg_amount": {"function": "AVG", "field": "amount"},
        }, is_jsonb=True),

        # Use raw SQL for complex expressions
        "revenue_range": "MAX((data->'amount')::numeric) - MIN((data->'amount')::numeric)",
        "active_count": "COUNT(*) FILTER (WHERE status = 'active')",
    }

    result = await db.aggregate("v_orders", aggregations=aggregates)

    print(f"Order Count: {result['count']}")
    print(f"Total Revenue: ${result['sum_amount']:,.2f}")
    print(f"Average Order: ${result['avg_amount']:,.2f}")
    print(f"Revenue Range: ${result['revenue_range']:,.2f}")
    print(f"Active Orders: {result['active_count']}")
    print()


async def example_time_based_filtering(db):
    """Example 5: Time-based aggregates."""
    print("Example 5: Time-Based Aggregates")
    print("=" * 50)

    # Last 30 days revenue
    aggregates = build_aggregate_dict({
        "recent_count": {"function": "COUNT"},
        "recent_revenue": {"function": "SUM", "field": "amount"},
    }, is_jsonb=True)

    result = await db.aggregate(
        "v_orders",
        aggregations=aggregates,
        where={
            "created_at": {"gte": "2026-01-01"},
            "status": {"eq": "completed"}
        }
    )

    print(f"Recent Orders (30d): {result['recent_count']}")
    print(f"Recent Revenue (30d): ${result['recent_revenue']:,.2f}")
    print()


async def example_statistical_aggregates(db):
    """Example 6: Statistical aggregates."""
    print("Example 6: Statistical Aggregates")
    print("=" * 50)

    aggregates = build_aggregate_dict({
        "order_count": {"function": "COUNT"},
        "avg_amount": {"function": "AVG", "field": "amount"},
        "stddev_amount": {"function": "STDDEV", "field": "amount"},
        "variance_amount": {"function": "VARIANCE", "field": "amount"},
    }, is_jsonb=True)

    result = await db.aggregate("v_orders", aggregations=aggregates)

    print(f"Order Count: {result['order_count']}")
    print(f"Average Amount: ${result['avg_amount']:,.2f}")
    print(f"Std Deviation: ${result['stddev_amount']:,.2f}")
    print(f"Variance: ${result['variance_amount']:,.2f}")
    print()


async def example_hybrid_table(db):
    """Example 7: Aggregates on hybrid tables (SQL + JSONB)."""
    print("Example 7: Hybrid Table Aggregates")
    print("=" * 50)

    # Assuming hybrid table with:
    # - SQL columns: id, created_at, status
    # - JSONB column: data (contains amount, customer_id, etc.)

    aggregates = {
        # SQL columns (no casting)
        **build_aggregate_dict({
            "total": {"function": "COUNT"},
        }, is_jsonb=False),

        # JSONB columns (with casting)
        **build_aggregate_dict({
            "sum_amount": {"function": "SUM", "field": "amount"},
            "avg_amount": {"function": "AVG", "field": "amount"},
        }, is_jsonb=True, jsonb_column="data"),
    }

    result = await db.aggregate("v_hybrid_orders", aggregations=aggregates)

    print(f"Total Orders: {result['total']}")
    print(f"Total Revenue: ${result['sum_amount']:,.2f}")
    print(f"Average Order: ${result['avg_amount']:,.2f}")
    print()


async def main():
    """Run all examples."""
    # Note: This is a demonstration file
    # In production, replace with your actual database connection

    print("FraiseQL Aggregation Examples")
    print("=" * 50)
    print()

    # Mock database object for demonstration
    # In production: db = await get_fraiseql_repository()
    class MockDB:
        async def aggregate(self, view_name, aggregations, where=None):
            # Mock implementation
            print(f"Executing on view: {view_name}")
            print(f"Aggregations: {list(aggregations.keys())}")
            if where:
                print(f"WHERE clause: {where}")
            print()
            # Return mock data
            return {
                key: 100 if "count" in key.lower() else 1000.0
                for key in aggregations.keys()
            }

    db = MockDB()

    # Run examples
    await example_basic_aggregates(db)
    await example_filtered_aggregates(db)
    await example_distinct_aggregates(db)
    await example_mixed_aggregates(db)
    await example_time_based_filtering(db)
    await example_statistical_aggregates(db)
    await example_hybrid_table(db)

    print("=" * 50)
    print("Examples completed!")
    print()
    print("Next steps:")
    print("1. Connect to your actual database")
    print("2. Ensure your views have JSONB 'data' column")
    print("3. Run aggregations with real data")
    print("4. Create functional indexes for performance:")
    print("   CREATE INDEX idx_orders_amount ON orders (((data->'amount')::numeric));")


if __name__ == "__main__":
    asyncio.run(main())
