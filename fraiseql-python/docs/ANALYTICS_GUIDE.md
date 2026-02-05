# Analytics Guide: Fact Tables and OLAP Queries

## Overview

FraiseQL provides high-performance analytics through **fact tables** and **aggregate queries**. This guide explains the pattern and how to use it.

## The Pattern

Fact tables follow a three-column pattern optimized for analytics:

```
┌──────────────────────────────────────┐
│        tf_sales Table                │
├──────────────────────────────────────┤
│ Measures: revenue, quantity, cost    │ ← Numeric columns for fast aggregation
│ Dimensions: data (JSONB)             │ ← Flexible GROUP BY via JSON
│ Filters: customer_id, occurred_at    │ ← Indexed columns for fast WHERE
└──────────────────────────────────────┘
```

## Why This Pattern?

**Traditional OLAP** (dimensional modeling):

```
Fact Table ──→ Dimension Table (customers)
           ──→ Dimension Table (products)
           ──→ Dimension Table (dates)
```

Problems:

- Lots of joins (slow)
- Schema flexibility (hard to add columns)
- Data duplication across dimensions

**FraiseQL Fact Tables** (denormalized):

```
Fact Table with:
  - Numeric measures (sum, avg, count)
  - JSONB dimensions (flexible schema)
  - Indexed filter columns (fast WHERE)
  - No joins needed!
```

Benefits:

- **Fast**: No expensive joins
- **Flexible**: JSONB allows adding dimensions without schema changes
- **Simple**: All data in one table

## Creating a Fact Table

### Step 1: Define the SQL Table

```sql
CREATE TABLE tf_sales (
    -- Row identifier
    id BIGSERIAL PRIMARY KEY,

    -- MEASURES: Numeric columns for aggregation
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,

    -- DIMENSIONS: JSONB for flexible GROUP BY
    data JSONB NOT NULL,  -- Contains: category, region, product, etc.

    -- DENORMALIZED FILTERS: Indexed for fast WHERE clauses
    customer_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,

    -- Audit columns (optional)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create indexes for filter performance
CREATE INDEX idx_sales_customer_id ON tf_sales(customer_id);
CREATE INDEX idx_sales_occurred_at ON tf_sales(occurred_at);
CREATE INDEX idx_sales_data ON tf_sales USING GIN(data);
```

### Step 2: Define the FraiseQL Type

```python
import fraiseql

@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity", "cost"],
    dimension_column="data",
    dimension_paths=[
        {
            "name": "category",
            "json_path": "data->>'category'",
            "data_type": "text"
        },
        {
            "name": "region",
            "json_path": "data->>'region'",
            "data_type": "text"
        },
        {
            "name": "product",
            "json_path": "data->>'product'",
            "data_type": "text"
        }
    ]
)
@fraiseql.type
class Sale:
    """A sales transaction fact."""
    id: int
    revenue: float          # Measure
    quantity: int           # Measure
    cost: float             # Measure
    customer_id: str        # Denormalized filter
    occurred_at: str        # Denormalized filter
```

### Step 3: Define an Aggregate Query

```python
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def sales_aggregate() -> list[dict]:
    """Aggregate sales with flexible grouping and filtering."""
    pass
```

## Populating Fact Tables

### ETL Pattern

Use your data pipeline to denormalize dimensions:

```python
# Pseudo-code for ETL
def etl_sales():
    # Read from transactional tables
    sales = query("""
        SELECT
            s.id, s.revenue, s.quantity, s.cost,
            c.id as customer_id,
            s.occurred_at,
            jsonb_build_object(
                'category', p.category,
                'region', c.region,
                'product', p.name
            ) as data
        FROM sales s
        JOIN customers c ON s.customer_id = c.id
        JOIN products p ON s.product_id = p.id
    """)

    # Write to fact table
    for row in sales:
        insert("""
            INSERT INTO tf_sales
            (revenue, quantity, cost, customer_id, occurred_at, data)
            VALUES (:revenue, :quantity, :cost, :customer_id, :occurred_at, :data)
        """, row)
```

## Query Examples

Once defined, you can query with:

```python
# GraphQL Query
query {
  salesAggregate {
    # GROUP BY dimensions
    category
    region
    month: occurredAtMonth

    # Aggregates
    revenueSum: revenueSumAgg
    revenuAvg: revenueAvgAgg
    quantitySum: quantitySumAgg
    count
  }
}
```

Generated dimensions:

- `category` - From data->>'category'
- `region` - From data->>'region'
- `product` - From data->>'product'
- `occurred_at_day` - Day bucket of occurred_at
- `occurred_at_month` - Month bucket of occurred_at
- `occurred_at_year` - Year bucket of occurred_at

Generated aggregates:

- `revenue_sum`, `revenue_avg`, `revenue_min`, `revenue_max`
- `quantity_sum`, `quantity_avg`, `quantity_min`, `quantity_max`
- `cost_sum`, `cost_avg`, `cost_min`, `cost_max`
- `count` - Number of rows

## Advanced: Multiple Fact Tables

Define multiple fact tables for different analytics:

```python
# Sales fact table
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity"],
    dimension_paths=[...]
)
@fraiseql.type
class SaleFact:
    id: int
    revenue: float
    quantity: int
    # ...

# Inventory fact table
@fraiseql.fact_table(
    table_name="tf_inventory",
    measures=["quantity_on_hand", "value"],
    dimension_paths=[...]
)
@fraiseql.type
class InventoryFact:
    id: int
    quantity_on_hand: int
    value: float
    # ...

# Define aggregate queries for each
@fraiseql.aggregate_query(fact_table="tf_sales")
@fraiseql.query
def sales_aggregate() -> list[dict]:
    pass

@fraiseql.aggregate_query(fact_table="tf_inventory")
@fraiseql.query
def inventory_aggregate() -> list[dict]:
    pass
```

## Performance Tuning

### Indexing Strategy

```sql
-- Fast measure filtering
CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(occurred_at);

-- Fast JSONB querying
CREATE INDEX ON tf_sales USING GIN(data);

-- Composite index for common filters
CREATE INDEX ON tf_sales(customer_id, occurred_at);
```

### Materialized Views

For frequently-accessed aggregations, create materialized views:

```sql
CREATE MATERIALIZED VIEW mv_sales_by_category AS
SELECT
    data->>'category' as category,
    DATE_TRUNC('month', occurred_at) as month,
    COUNT(*) as count,
    SUM(revenue) as revenue_total
FROM tf_sales
GROUP BY data->>'category', DATE_TRUNC('month', occurred_at);

CREATE INDEX ON mv_sales_by_category(category, month);
```

Then query the materialized view in your aggregate queries.

### Partitioning

For large fact tables, partition by time:

```sql
-- Create partitioned table
CREATE TABLE tf_sales (
    id BIGSERIAL,
    revenue DECIMAL(10,2),
    quantity INT,
    data JSONB,
    customer_id UUID,
    occurred_at TIMESTAMPTZ,
    PRIMARY KEY (id, occurred_at)
) PARTITION BY RANGE (EXTRACT(YEAR FROM occurred_at));

-- Create partitions
CREATE TABLE tf_sales_2024
    PARTITION OF tf_sales
    FOR VALUES FROM (2024) TO (2025);

CREATE TABLE tf_sales_2025
    PARTITION OF tf_sales
    FOR VALUES FROM (2025) TO (2026);
```

## Limitations

- ❌ No joins between fact tables (use separate queries)
- ❌ No real-time aggregates (use ETL pipeline)
- ❌ Limited to one dimension JSONB column
- ⚠️ JSONB performance degrades with very wide schemas (100+ dimensions)

## Best Practices

1. **Keep measures simple**: Only numeric columns that aggregate meaningfully
2. **Limit dimensions**: 20-30 dimensions typically sufficient
3. **Denormalize aggressively**: Flatten data in ETL, not at query time
4. **Index filters**: Index customer_id, date ranges, and frequently-used dimensions
5. **Partition by time**: Monthly or yearly partitions for large tables
6. **Use materialized views**: For commonly-run aggregations
7. **Archive old data**: Move historical data to separate tables
8. **Monitor cardinality**: JSONB with extremely high cardinality values can slow queries

## Real-World Example

Complete e-commerce sales analytics:

```python
import fraiseql

# Define the fact table
@fraiseql.fact_table(
    table_name="tf_orders",
    measures=["subtotal", "tax", "shipping", "total"],
    dimension_paths=[
        {"name": "product_category", "json_path": "data->>'product_category'", "data_type": "text"},
        {"name": "product_name", "json_path": "data->>'product_name'", "data_type": "text"},
        {"name": "customer_country", "json_path": "data->>'customer_country'", "data_type": "text"},
        {"name": "customer_segment", "json_path": "data->>'customer_segment'", "data_type": "text"}
    ]
)
@fraiseql.type
class OrderFact:
    """Sales order fact table."""
    id: int
    subtotal: float
    tax: float
    shipping: float
    total: float
    customer_id: str
    occurred_at: str

# Define aggregate queries
@fraiseql.aggregate_query(fact_table="tf_orders", auto_group_by=True, auto_aggregates=True)
@fraiseql.query
def orders_by_category() -> list[dict]:
    """Orders aggregated by product category."""
    pass

@fraiseql.aggregate_query(fact_table="tf_orders", auto_group_by=True, auto_aggregates=True)
@fraiseql.query
def orders_by_country() -> list[dict]:
    """Orders aggregated by customer country."""
    pass

@fraiseql.aggregate_query(fact_table="tf_orders", auto_group_by=True, auto_aggregates=True)
@fraiseql.query
def orders_by_month() -> list[dict]:
    """Orders aggregated by month."""
    pass

# Export
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

This generates three flexible aggregate queries supporting grouping by dimensions, filtering, and aggregation functions - all without writing custom resolvers!
