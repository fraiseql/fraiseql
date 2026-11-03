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

### Step 2: Declare the Fact Table

```python
import fraiseql

@fraiseql.type
class Sale:
    """A sales transaction fact."""
    revenue: float          # Measure
    quantity: int           # Measure
    cost: float             # Measure
    category: str           # Dimension path
    region: str             # Dimension path
    product: str            # Dimension path
    customer_id: str        # Denormalized filter
    occurred_at: str        # Denormalized filter

fraiseql.fact_table(
    "tf_sales",
    type_name="Sale",
    measures=[
        {"name": "revenue", "sql_type": "numeric", "nullable": False},
        {"name": "quantity", "sql_type": "int", "nullable": False},
        {"name": "cost", "sql_type": "numeric", "nullable": False},
    ],
    dimensions={
        "name": "data",
        "paths": [
            {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
            {"name": "region", "json_path": "data->>'region'", "data_type": "text"},
            {"name": "product", "json_path": "data->>'product'", "data_type": "text"},
        ],
    },
    denormalized_filters=[
        {"name": "customer_id", "sql_type": "uuid", "indexed": True},
        {"name": "occurred_at", "sql_type": "timestamptz", "indexed": True},
    ],
)
```

`fraiseql.fact_table()` is a plain function, not a decorator. `type_name` links the fact
table to the type it is read as: every measure, denormalized filter and dimension path must
be a field of that type, and the type's read gates apply to aggregates over it. Omit
`type_name` when no type is needed.

### Step 3: Query It

There is no aggregate query to declare. A fact table `tf_<name>` gets two GraphQL roots,
`<name>_aggregate` and `<name>_window`, derived when the schema compiles. For `tf_sales`
these are `sales_aggregate` and `sales_window`.

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

Once declared, you can query with:

```graphql
query {
  sales_aggregate(
    groupBy: { category: true, region: true, occurred_at: "month" }
    aggregates: [{ revenue_sum: {} }, { revenue_avg: {} }, { quantity_sum: {} }, { count: {} }]
  )
}
```

A time bucket is spelled either `groupBy: { occurred_at: "month" }` or
`groupBy: { occurred_at_month: true }`.

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

Declare one fact table per analytics relation; each gets its own roots:

```python
# Sales fact table: roots sales_aggregate and sales_window
fraiseql.fact_table(
    "tf_sales",
    measures=[
        {"name": "revenue", "sql_type": "numeric", "nullable": False},
        {"name": "quantity", "sql_type": "int", "nullable": False},
    ],
    dimensions={"name": "data", "paths": [...]},
)

# Inventory fact table: roots inventory_aggregate and inventory_window
fraiseql.fact_table(
    "tf_inventory",
    measures=[
        {"name": "quantity_on_hand", "sql_type": "int", "nullable": False},
        {"name": "value", "sql_type": "numeric", "nullable": False},
    ],
    dimensions={"name": "data", "paths": [...]},
)
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

To aggregate over the view itself, name it `tf_<name>` and declare it with `fraiseql.fact_table()`; its `<name>_aggregate` root then reads it.

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

# Declare the fact table: roots orders_aggregate and orders_window
fraiseql.fact_table(
    "tf_orders",
    measures=[
        {"name": "subtotal", "sql_type": "numeric", "nullable": False},
        {"name": "tax", "sql_type": "numeric", "nullable": False},
        {"name": "shipping", "sql_type": "numeric", "nullable": False},
        {"name": "total", "sql_type": "numeric", "nullable": False},
    ],
    dimensions={
        "name": "data",
        "paths": [
            {"name": "product_category", "json_path": "data->>'product_category'", "data_type": "text"},
            {"name": "product_name", "json_path": "data->>'product_name'", "data_type": "text"},
            {"name": "customer_country", "json_path": "data->>'customer_country'", "data_type": "text"},
            {"name": "customer_segment", "json_path": "data->>'customer_segment'", "data_type": "text"},
        ],
    },
    denormalized_filters=[
        {"name": "customer_id", "sql_type": "uuid", "indexed": True},
        {"name": "occurred_at", "sql_type": "timestamptz", "indexed": True},
    ],
)

# Export
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

Orders by category, by country and by month are three requests to the one derived root:

```graphql
{ orders_aggregate(groupBy: { product_category: true }, aggregates: [{ total_sum: {} }, { count: {} }]) }
{ orders_aggregate(groupBy: { customer_country: true }, aggregates: [{ total_sum: {} }, { count: {} }]) }
{ orders_aggregate(groupBy: { occurred_at: "month" }, aggregates: [{ total_sum: {} }, { count: {} }]) }
```
