<!-- Skip to main content -->
---
title: View Selection API: Explicit View Control
description: FraiseQL allows developers to explicitly control which view backs each GraphQL type or Arrow Flight ticket. This document explains the API patterns for view sel
keywords: ["directives", "types", "scalars", "schema", "api"]
tags: ["documentation", "reference"]
---

# View Selection API: Explicit View Control

## Overview

FraiseQL allows developers to explicitly control which view backs each GraphQL type or Arrow Flight ticket. This document explains the API patterns for view selection in both planes.

**Core principle**: View selection is **explicit**, not automatic. Developers specify which view to use; FraiseQL handles compilation.

## JSON Plane (GraphQL)

### View Binding in Schema Authoring

In the authoring layer (Python/TypeScript), bind types to specific views using the `view` parameter:

```python
<!-- Code example in Python -->
import FraiseQL

# Default: Uses v_user (logical view)
@FraiseQL.type()
class User:
    id: str
    name: str
    email: str

# Explicit: Uses v_user (same as default, for clarity)
@FraiseQL.type(view="v_user")
class User:
    id: str
    name: str
    email: str

# Table-backed: Uses tv_user_profile (pre-computed JSONB)
@FraiseQL.type(view="tv_user_profile")
class UserProfile:
    id: str
    name: str
    email: str
    posts: list[Post]
    comments: list[Comment]
    likes: list[Like]
```text
<!-- Code example in TEXT -->

### Schema Compilation

When you compile the schema, FraiseQL reads the `view` parameter and validates it exists:

```bash
<!-- Code example in BASH -->
# Authoring phase (generates schema.json)
python -m FraiseQL generate-schema

# Compilation phase (validates views and generates executors)
FraiseQL-cli compile schema.json --validate

# The compiler checks:
# 1. View exists in database
# 2. View has required columns (id, data, etc.)
# 3. data JSONB matches type definition
```text
<!-- Code example in TEXT -->

### Configuration in schema.json

After compilation, the schema includes explicit view references:

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "User",
      "view": "v_user",
      "fields": [
        { "name": "id", "type": "String" },
        { "name": "name", "type": "String" },
        { "name": "email", "type": "String" }
      ]
    },
    {
      "name": "UserProfile",
      "view": "tv_user_profile",
      "fields": [
        { "name": "id", "type": "String" },
        { "name": "name", "type": "String" },
        { "name": "email", "type": "String" },
        { "name": "posts", "type": "[Post]" },
        { "name": "comments", "type": "[Comment]" },
        { "name": "likes", "type": "[Like]" }
      ]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Client Usage

Clients don't select views; they query types. The view is determined at schema definition time:

```graphql
<!-- Code example in GraphQL -->
# Client query
query {
  # Simple query uses v_user (fast)
  user(id: "550e8400...") {
    id
    name
    email
  }

  # Complex query uses tv_user_profile (if available)
  userProfile(id: "550e8400...") {
    id
    name
    email
    posts {
      id
      title
    }
    comments {
      id
      text
    }
  }
}
```text
<!-- Code example in TEXT -->

### Python Example

```python
<!-- Code example in Python -->
from FraiseQL import type, schema

# Define both simple and complex types
@type()
class User:
    """Simple user view"""
    id: str
    name: str
    email: str

@type(view="tv_user_profile")
class UserWithNested:
    """Complex user profile with nested data"""
    id: str
    name: str
    email: str
    posts: list['Post']
    comments: list['Comment']
    friends: list['User']

@type()
class Post:
    id: str
    title: str
    content: str
    user_id: str

@type()
class Comment:
    id: str
    text: str
    post_id: str

# Schema compilation
schema_file = schema.compile(
    types=[User, UserWithNested, Post, Comment],
    database_url="postgresql://localhost/fraiseql_dev"
)

# Generated schema.json:
# {
#   "types": [
#     { "name": "User", "view": "v_user", ... },
#     { "name": "UserWithNested", "view": "tv_user_profile", ... },
#     { "name": "Post", "view": "v_post", ... },
#     { "name": "Comment", "view": "v_comment", ... }
#   ]
# }
```text
<!-- Code example in TEXT -->

### TypeScript Example

```typescript
<!-- Code example in TypeScript -->
import { type, schema } from "@FraiseQL/typescript";

// Define both simple and complex types
@type()
class User {
  id: string;
  name: string;
  email: string;
}

@type({ view: "tv_user_profile" })
class UserWithNested {
  id: string;
  name: string;
  email: string;
  posts: Post[];
  comments: Comment[];
  friends: User[];
}

@type()
class Post {
  id: string;
  title: string;
  content: string;
  userId: string;
}

// Schema compilation
const schemaFile = schema.compile({
  types: [User, UserWithNested, Post],
  databaseUrl: "postgresql://localhost/fraiseql_dev"
});
```text
<!-- Code example in TEXT -->

## Arrow Flight (Analytics Plane)

### Explicit View Selection in Tickets

Clients explicitly select which view to query by including it in the Flight ticket:

```python
<!-- Code example in Python -->
import pyarrow.flight as flight
import json

client = flight.connect("grpc://localhost:50051")

# Use logical view for small analytics
ticket_logical = {
    "view": "va_orders",      # Explicit view selection
    "limit": 10000,
    "filter": "status = 'completed'"
}
stream = client.do_get(flight.Ticket(json.dumps(ticket_logical).encode()))
records = stream.read_all()

# Use table-backed view for large analytics
ticket_table = {
    "view": "ta_orders",      # Explicit view selection
    "limit": 1000000,
    "filter": "created_at >= '2026-01-01'"
}
stream = client.do_get(flight.Ticket(json.dumps(ticket_table).encode()))
records = stream.read_all()
```text
<!-- Code example in TEXT -->

### Ticket Structure

```json
<!-- Code example in JSON -->
{
  "view": "ta_orders",
  "limit": 100000,
  "offset": 0,
  "filter": "created_at >= '2026-01-01' AND status = 'completed'",
  "order_by": "created_at DESC",
  "columns": ["id", "total", "created_at", "customer_name"],
  "metadata": {
    "format": "arrow_ipc",
    "compression": "zstd"
  }
}
```text
<!-- Code example in TEXT -->

**Fields**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `view` | string | ✅ Yes | View name: `va_*` or `ta_*` |
| `limit` | integer | ❌ No | Max rows to return (default: 10000) |
| `offset` | integer | ❌ No | Skip first N rows (default: 0) |
| `filter` | string | ❌ No | SQL WHERE clause (without WHERE) |
| `order_by` | string | ❌ No | SQL ORDER BY (without ORDER BY) |
| `columns` | array | ❌ No | Subset of columns to return |
| `metadata` | object | ❌ No | Format and compression options |

### Python Example

```python
<!-- Code example in Python -->
import pyarrow.flight as flight
import json
import pandas as pd

def query_orders_logical():
    """Small dataset using logical view"""
    client = flight.connect("grpc://localhost:50051")

    ticket = {
        "view": "va_orders",
        "limit": 10000,
        "filter": "created_at >= '2026-01-01'",
        "order_by": "created_at DESC"
    }

    stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
    table = stream.read_all()
    df = table.to_pandas()

    return df

def query_orders_table():
    """Large dataset using table-backed view"""
    client = flight.connect("grpc://localhost:50051")

    ticket = {
        "view": "ta_orders",          # Use table-backed view
        "limit": 1000000,
        "filter": "created_at >= '2025-01-01'",
        "order_by": "created_at DESC",
        "columns": ["id", "total", "created_at"],
        "metadata": {
            "compression": "zstd"     # Enable compression for large transfers
        }
    }

    stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
    table = stream.read_all()
    df = table.to_pandas()

    return df

# Usage
df_small = query_orders_logical()  # 100-200ms
df_large = query_orders_table()    # 50-100ms (50M rows)
```text
<!-- Code example in TEXT -->

### View Discovery

Clients can discover available views before querying:

```python
<!-- Code example in Python -->
import pyarrow.flight as flight

client = flight.connect("grpc://localhost:50051")

# List all available flights/views
flights = client.list_flights()

for flight_info in flights:
    view_name = flight_info.name
    print(f"View: {view_name}")

    # Determine view type
    if view_name.startswith("ta_"):
        print(f"  Type: table-backed (Arrow plane)")
    elif view_name.startswith("va_"):
        print(f"  Type: logical (Arrow plane)")
    elif view_name.startswith("tv_"):
        print(f"  Type: table-backed (JSON plane - not for Arrow Flight)")
    elif view_name.startswith("v_"):
        print(f"  Type: logical (JSON plane - not for Arrow Flight)")

    # Get schema
    schema = client.get_schema(flight_info)
    print(f"  Schema: {schema}")
    print(f"  Rows: {flight_info.total_records}")
```text
<!-- Code example in TEXT -->

### When to Use Each View

```python
<!-- Code example in Python -->
def should_use_table_backed(dataset_size: int, query_time_ms: float) -> bool:
    """Decision logic for choosing between logical and table-backed views"""
    # Rule 1: Large datasets (>1M rows) → use table-backed
    if dataset_size > 1_000_000:
        return True

    # Rule 2: Slow queries (>1 second) → use table-backed
    if query_time_ms > 1000:
        return True

    # Rule 3: Time-series with range queries → use table-backed
    if has_time_series_filter:
        return True

    # Otherwise, use logical view
    return False

# Usage
if should_use_table_backed(dataset_size=10_000_000, query_time_ms=5000):
    ticket = {"view": "ta_orders"}  # Use ta_*
else:
    ticket = {"view": "va_orders"}  # Use va_*
```text
<!-- Code example in TEXT -->

## View Validation

### At Compilation Time

The compiler validates that all views exist and have correct schemas:

```bash
<!-- Code example in BASH -->
FraiseQL-cli compile schema.json --validate

# Checks:
# ✅ v_user exists in database
# ✅ v_user has columns: id, data
# ✅ v_user.data JSONB contains User fields
# ✅ tv_user_profile exists in database
# ✅ tv_user_profile has columns: id, data
# ✅ ta_orders exists in database
# ✅ ta_orders has Arrow-compatible columns
```text
<!-- Code example in TEXT -->

### At Query Time

The runtime validates ticket requests:

```python
<!-- Code example in Python -->
client.do_get(flight.Ticket(json.dumps({
    "view": "nonexistent_view",  # Runtime error
    "limit": 10000
}).encode()))

# Error: View 'nonexistent_view' not found
# Available views: [ta_orders, va_orders, ta_users, va_users]
```text
<!-- Code example in TEXT -->

## API Documentation

### GraphQL Type Binding

**Function**: `@FraiseQL.type(view: str = None)`

**Parameters**:

- `view` (optional): View name to use (e.g., "v_user", "tv_user_profile")
- Default: Infers from type name (e.g., `User` → `v_user`)

**Usage**:

```python
<!-- Code example in Python -->
@FraiseQL.type()                    # Uses v_user by default
class User:
    pass

@FraiseQL.type(view="tv_user")      # Explicit view
class User:
    pass
```text
<!-- Code example in TEXT -->

### Arrow Flight Ticket

**Structure**:

```typescript
<!-- Code example in TypeScript -->
interface FlightTicket {
  view: string;           // Required: view name
  limit?: number;         // Optional: max rows
  offset?: number;        // Optional: skip rows
  filter?: string;        // Optional: WHERE clause
  order_by?: string;      // Optional: ORDER BY
  columns?: string[];     // Optional: subset of columns
  metadata?: {
    format?: string;      // arrow_ipc, parquet
    compression?: string; // zstd, snappy, gzip
  };
}
```text
<!-- Code example in TEXT -->

## Error Handling

### View Not Found

```python
<!-- Code example in Python -->
# Error case: view doesn't exist
ticket = {"view": "ta_nonexistent"}
try:
    stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
except flight.FlightError as e:
    print(f"Error: {e}")
    # Error: View 'ta_nonexistent' not found
    # Available: [ta_orders, ta_users, va_orders]
```text
<!-- Code example in TEXT -->

### Invalid Filter

```python
<!-- Code example in Python -->
# Error case: invalid SQL filter
ticket = {
    "view": "ta_orders",
    "filter": "invalid SQL HERE"
}
try:
    stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
except flight.FlightError as e:
    print(f"Error: {e}")
    # Error: Invalid filter syntax
```text
<!-- Code example in TEXT -->

### Type Mismatch (GraphQL)

```python
<!-- Code example in Python -->
# Error case: type doesn't match view
@FraiseQL.type(view="ta_orders")  # ta_* is Arrow-only!
class Order:
    id: str

# Compilation error: 'ta_orders' is table-backed Arrow view
# Cannot use for JSON plane type
```text
<!-- Code example in TEXT -->

## Best Practices

### 1. Be Explicit About View Choice

```python
<!-- Code example in Python -->
# ✅ Good: Clear intent
@FraiseQL.type(view="tv_user_profile")
class UserWithPosts:
    """User profile with nested posts and comments"""
    pass

# ❌ Bad: Ambiguous
@FraiseQL.type()
class User:
    """User"""
    pass
```text
<!-- Code example in TEXT -->

### 2. Document Why You're Using a Specific View

```python
<!-- Code example in Python -->
@FraiseQL.type(view="tv_order_summary")
class OrderSummary:
    """Order with line items and customer details.

    Using tv_order_summary (table-backed) because:
    - Complex nesting (3+ joins)
    - High read volume (>100 req/sec)
    - Typical query time: 2-5s with v_order, 100ms with tv_
    """
    pass
```text
<!-- Code example in TEXT -->

### 3. Test Performance Before and After Migration

```python
<!-- Code example in Python -->
import time

def benchmark_view(view_name: str, query_count: int = 100):
    """Benchmark query performance"""
    client = flight.connect("grpc://localhost:50051")
    ticket = {"view": view_name, "limit": 10000}

    start = time.time()
    for _ in range(query_count):
        stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
        _ = stream.read_all()

    elapsed = time.time() - start
    avg_ms = (elapsed / query_count) * 1000

    print(f"{view_name}: {avg_ms:.1f}ms avg")
    return avg_ms

# Test both views
t_logical = benchmark_view("va_orders")
t_table = benchmark_view("ta_orders")

if t_table < t_logical * 0.5:
    print(f"✅ Migration saves {(1 - t_table/t_logical)*100:.0f}%")
else:
    print(f"❌ Migration not worth it")
```text
<!-- Code example in TEXT -->

### 4. Start with Logical Views

```python
<!-- Code example in Python -->
# ✅ Good: Start simple
@FraiseQL.type()
class Order:
    pass

# Later, if performance requires:
@FraiseQL.type(view="tv_order_with_items")
class OrderWithItems:
    pass
```text
<!-- Code example in TEXT -->

### 5. Monitor View Freshness

For table-backed views, monitor staleness:

```sql
<!-- Code example in SQL -->
-- Check how old the data is
SELECT
    view_name,
    MAX(updated_at) - NOW() as staleness,
    COUNT(*) as row_count
FROM (
    SELECT 'tv_user_profile' as view_name, updated_at FROM tv_user_profile
    UNION
    SELECT 'tv_order_summary', updated_at FROM tv_order_summary
) views
GROUP BY view_name;
```text
<!-- Code example in TEXT -->

## See Also

- [View Selection Guide](../architecture/database/view-selection-guide.md)
- [tv_* Table Pattern](../architecture/database/tv-table-pattern.md)
- [ta_* Table Pattern](../architecture/database/ta-table-pattern.md)
- [Schema Conventions](../specs/schema-conventions.md)
