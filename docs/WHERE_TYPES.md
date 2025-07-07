# WHERE Types Guide for FraiseQL

FraiseQL provides a powerful and type-safe way to generate SQL WHERE clauses from GraphQL queries using the `safe_create_where_type` pattern. This guide covers all supported operators, complex filtering scenarios, and best practices.

## Table of Contents

- [Overview](#overview)
- [Basic Usage](#basic-usage)
- [Supported Operators](#supported-operators)
- [Complex Filtering Examples](#complex-filtering-examples)
- [Automatic Type Casting](#automatic-type-casting)
- [SQL Injection Prevention](#sql-injection-prevention)
- [Integration with GraphQL](#integration-with-graphql)
- [Best Practices](#best-practices)

## Overview

WHERE types in FraiseQL are dynamically generated dataclasses that translate GraphQL filter inputs into parameterized SQL WHERE clauses. They provide:

- **Type safety** - Filters are validated against your Python type annotations
- **SQL injection protection** - All values are properly parameterized
- **Automatic type casting** - JSONB text values are cast to appropriate SQL types
- **Rich operator support** - From simple equality to complex pattern matching

## Basic Usage

### Creating a WHERE Type

```python
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from uuid import UUID
from fraiseql.sql.where_generator import safe_create_where_type

@dataclass
class Product:
    id: UUID
    name: str
    price: Decimal
    in_stock: bool
    created_at: datetime
    tags: list[str]

# Create the WHERE type
ProductWhere = safe_create_where_type(Product)
```

### Using in Queries

```python
@fraiseql.query
async def products(
    info,
    where: ProductWhere | None = None
) -> list[Product]:
    db = info.context["db"]
    return await db.find("product_view", where=where)
```

### GraphQL Query Examples

```graphql
# Simple equality
query {
  products(where: { name: { eq: "iPhone 15" } }) {
    id
    name
    price
  }
}

# Range queries
query {
  products(where: {
    price: { gte: 100, lte: 1000 }
    in_stock: { eq: true }
  }) {
    name
    price
  }
}
```

## Supported Operators

### Comparison Operators

| Operator | SQL Equivalent | Example | Description |
|----------|---------------|---------|-------------|
| `eq` | `=` | `{ price: { eq: 99.99 } }` | Exact equality |
| `neq` | `!=` | `{ status: { neq: "deleted" } }` | Not equal |
| `gt` | `>` | `{ age: { gt: 18 } }` | Greater than |
| `gte` | `>=` | `{ rating: { gte: 4.0 } }` | Greater than or equal |
| `lt` | `<` | `{ stock: { lt: 10 } }` | Less than |
| `lte` | `<=` | `{ price: { lte: 100 } }` | Less than or equal |

### String Operators

| Operator | SQL Equivalent | Example | Description |
|----------|---------------|---------|-------------|
| `contains` | `ILIKE %value%` | `{ name: { contains: "pro" } }` | Case-insensitive substring match |
| `startswith` | `LIKE value%` | `{ email: { startswith: "admin" } }` | Starts with prefix |
| `matches` | `~` | `{ code: { matches: "^[A-Z]{3}-\\d+$" } }` | Regex pattern match |

### List Operators

| Operator | SQL Equivalent | Example | Description |
|----------|---------------|---------|-------------|
| `in` | `IN` | `{ status: { in: ["active", "pending"] } }` | Value in list |
| `notin` | `NOT IN` | `{ category: { notin: ["archived", "deleted"] } }` | Value not in list |

### Null Handling

| Operator | SQL Equivalent | Example | Description |
|----------|---------------|---------|-------------|
| `isnull` | `IS NULL` / `IS NOT NULL` | `{ deleted_at: { isnull: true } }` | Check for null values |

### Advanced Operators (PostgreSQL-specific)

| Operator | SQL Equivalent | Example | Description |
|----------|---------------|---------|-------------|
| `overlaps` | `&&` | `{ tags: { overlaps: ["tech", "mobile"] } }` | Array overlap |
| `strictly_contains` | `@> AND !=` | `{ path: { strictly_contains: "/users" } }` | Contains but not equal |

## Complex Filtering Examples

### 1. Multiple Conditions (AND Logic)

```python
# Find premium products in stock
where = ProductWhere(
    price={"gte": 500},
    in_stock={"eq": True},
    category={"eq": "electronics"}
)
# SQL: (data->>'price')::numeric >= 500
#      AND (data->>'in_stock')::boolean = true
#      AND (data->>'category') = 'electronics'
```

### 2. Range Queries

```python
# Find products in price range
where = ProductWhere(
    price={"gte": 100, "lte": 1000}
)
# SQL: (data->>'price')::numeric >= 100
#      AND (data->>'price')::numeric <= 1000
```

### 3. Pattern Matching

```python
# Find users with Gmail addresses
where = UserWhere(
    email={"matches": r".*@gmail\.com$"}
)
# SQL: (data->>'email') ~ '.*@gmail\.com$'

# Find products starting with "Pro"
where = ProductWhere(
    name={"startswith": "Pro"}
)
# SQL: (data->>'name') LIKE 'Pro%'
```

### 4. List Membership

```python
# Find orders with specific statuses
where = OrderWhere(
    status={"in": ["pending", "processing", "shipped"]}
)
# SQL: (data->>'status') IN ('pending', 'processing', 'shipped')

# Exclude certain categories
where = ProductWhere(
    category={"notin": ["discontinued", "internal"]}
)
# SQL: (data->>'category') NOT IN ('discontinued', 'internal')
```

### 5. Null Checking

```python
# Find users without profile pictures
where = UserWhere(
    avatar_url={"isnull": True}
)
# SQL: (data->>'avatar_url') IS NULL

# Find completed tasks
where = TaskWhere(
    completed_at={"isnull": False}
)
# SQL: (data->>'completed_at') IS NOT NULL
```

### 6. Date/Time Filtering

```python
from datetime import datetime, timedelta

# Find recent orders
cutoff = datetime.now() - timedelta(days=7)
where = OrderWhere(
    created_at={"gte": cutoff}
)
# SQL: (data->>'created_at')::timestamp >= '2024-01-01 00:00:00'

# Find birthdays in date range
where = UserWhere(
    birth_date={"gte": date(1990, 1, 1), "lt": date(2000, 1, 1)}
)
# SQL: (data->>'birth_date')::date >= '1990-01-01'
#      AND (data->>'birth_date')::date < '2000-01-01'
```

### 7. Complex Business Logic

```python
# Find high-value customers (multiple criteria)
where = CustomerWhere(
    total_purchases={"gte": 1000},
    account_status={"eq": "active"},
    email_verified={"eq": True},
    last_purchase_date={"gte": datetime.now() - timedelta(days=90)}
)
```

## Automatic Type Casting

FraiseQL automatically casts JSONB text values to appropriate PostgreSQL types based on Python type hints:

### Type Casting Rules

| Python Type | PostgreSQL Cast | Example |
|------------|----------------|---------|
| `int` | `::integer` | `(data->>'age')::integer > 18` |
| `float` | `::numeric` | `(data->>'rating')::numeric >= 4.5` |
| `Decimal` | `::numeric` | `(data->>'price')::numeric < 100` |
| `bool` | `::boolean` | `(data->>'is_active')::boolean = true` |
| `datetime` | `::timestamp` | `(data->>'created_at')::timestamp > '2024-01-01'` |
| `date` | `::date` | `(data->>'birth_date')::date <= '2000-12-31'` |
| `UUID` | `::uuid` | `(data->>'id')::uuid = '123e4567-...'` |
| `str` | No cast | `(data->>'name') = 'John'` |

### Important Notes on Type Casting

1. **Numeric comparisons**: Always cast to `::numeric` for proper ordering
2. **Boolean values**: Stored as JSON `true`/`false`, cast for comparison
3. **Dates/Times**: ISO 8601 format expected in JSONB
4. **Arrays**: Use JSONB operators like `@>` without casting

## SQL Injection Prevention

FraiseQL uses multiple layers of protection against SQL injection:

### 1. Parameterized Queries

All values are passed as parameters, never interpolated:

```python
# User input
where = ProductWhere(name={"eq": "'; DROP TABLE products; --"})

# Generated SQL (safe)
# SQL: (data->>'name') = %s
# Params: ["'; DROP TABLE products; --"]
```

### 2. Operator Validation

Only predefined operators are allowed:

```python
# This would raise an error
where = ProductWhere(name={"hack": "value"})  # Unknown operator
```

### 3. Column Name Validation

Column names come from your dataclass fields:

```python
# This would raise an error at type creation
where = ProductWhere(**{"__injected__": "value"})  # Invalid field
```

## Integration with GraphQL

### 1. Define WHERE Input Type

```python
@fraise_input
class ProductWhereInput:
    """GraphQL input type for product filtering."""
    name: StringFilter | None = None
    price: DecimalFilter | None = None
    in_stock: BooleanFilter | None = None
    created_at: DateTimeFilter | None = None

@fraise_input
class StringFilter:
    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    startswith: str | None = None
    in: list[str] | None = None
```

### 2. Use in Query

```python
@fraiseql.query
async def products(
    info,
    where: ProductWhereInput | None = None,
    order_by: str = "created_at",
    limit: int = 20
) -> list[Product]:
    db = info.context["db"]

    # Convert GraphQL input to WHERE type
    if where:
        where_clause = ProductWhere(**where.__dict__)
    else:
        where_clause = None

    return await db.find(
        "product_view",
        where=where_clause,
        order_by=order_by,
        limit=limit
    )
```

### 3. GraphQL Query

```graphql
query FilteredProducts {
  products(
    where: {
      price: { gte: 100, lte: 500 }
      in_stock: { eq: true }
      name: { contains: "Pro" }
    }
    order_by: "price"
    limit: 10
  ) {
    id
    name
    price
  }
}
```

## Best Practices

### 1. Use Type-Specific Operators

Choose operators that match your data type:

```python
# Good - using numeric comparison for numbers
where = ProductWhere(price={"gte": 100})

# Bad - using string contains for numbers
where = ProductWhere(price={"contains": "100"})  # Will fail
```

### 2. Index Your JSONB Columns

For performance, create appropriate indexes:

```sql
-- GIN index for JSONB containment queries
CREATE INDEX idx_products_data_gin ON products USING GIN (data);

-- B-tree index for specific fields
CREATE INDEX idx_products_price ON products ((data->>'price')::numeric);
CREATE INDEX idx_products_created ON products ((data->>'created_at')::timestamp);
```

### 3. Validate Input Ranges

Add business logic validation:

```python
@fraiseql.query
async def products(info, where: ProductWhereInput | None = None) -> list[Product]:
    # Validate price range
    if where and where.price:
        if where.price.gte and where.price.lte:
            if where.price.gte > where.price.lte:
                raise ValueError("Invalid price range")

    # ... rest of query
```

### 4. Use Meaningful Filter Names

Structure your WHERE types for clarity:

```python
@dataclass
class OrderFilters:
    # Status filters
    status: StatusFilter | None = None

    # Date range filters
    created_after: datetime | None = None
    created_before: datetime | None = None

    # Customer filters
    customer_id: UUID | None = None
    customer_email: str | None = None

    # Amount filters
    total_gte: Decimal | None = None
    total_lte: Decimal | None = None
```

### 5. Document Complex Filters

Add docstrings to explain filter behavior:

```python
@fraiseql.query
async def search_products(
    info,
    where: ProductWhereInput | None = None
) -> list[Product]:
    """Search products with advanced filtering.

    Filters:
    - name.contains: Case-insensitive substring match
    - price.gte/lte: Inclusive price range
    - tags.overlaps: Products with ANY of the specified tags
    - created_at.gte: Products created after this date
    """
    # Implementation
```

## Advanced Patterns

### 1. Combining Multiple WHERE Types

```python
@fraiseql.query
async def complex_search(
    info,
    product_where: ProductWhere | None = None,
    vendor_where: VendorWhere | None = None
) -> list[ProductWithVendor]:
    # Build complex JOIN query with multiple WHERE clauses
    # ...
```

### 2. Dynamic Filter Building

```python
def build_dynamic_where(filters: dict[str, Any]) -> ProductWhere:
    """Build WHERE clause from dynamic filter dict."""
    where_dict = {}

    for field, value in filters.items():
        if field == "price_min":
            where_dict["price"] = {"gte": value}
        elif field == "price_max":
            where_dict.setdefault("price", {})["lte"] = value
        elif field == "search":
            where_dict["name"] = {"contains": value}

    return ProductWhere(**where_dict)
```

### 3. Custom Operators

While FraiseQL provides standard operators, you can extend with custom logic:

```python
@fraiseql.query
async def products_near_location(
    info,
    latitude: float,
    longitude: float,
    radius_km: float = 10
) -> list[Product]:
    db = info.context["db"]

    # Custom PostGIS query
    sql = """
    SELECT data FROM product_view
    WHERE ST_DWithin(
        (data->>'location')::geography,
        ST_MakePoint(%s, %s)::geography,
        %s
    )
    """

    return await db.execute_raw(sql, [longitude, latitude, radius_km * 1000])
```

## Troubleshooting

### Common Issues

1. **Type casting errors**: Ensure your JSONB data matches expected types
2. **Missing indexes**: Add indexes for frequently filtered fields
3. **Complex queries slow**: Consider materialized views for complex filters
4. **Boolean comparisons failing**: Remember booleans are stored as JSON `true`/`false`

### Debugging WHERE Clauses

```python
# Log generated SQL
where = ProductWhere(price={"gte": 100})
sql = where.to_sql()
print(sql.as_string(None))  # See actual SQL
print(where._get_params())  # See parameters
```

## Summary

FraiseQL's WHERE types provide a powerful, type-safe way to build complex SQL queries from GraphQL inputs. With automatic type casting, comprehensive operator support, and built-in SQL injection prevention, you can build sophisticated filtering capabilities while maintaining security and performance.

For more examples and patterns, see:
- [Query Patterns](QUERY_PATTERNS.md)
- [Filtering Patterns](FILTERING_PATTERNS.md)
- [Common Patterns](COMMON_PATTERNS.md)
