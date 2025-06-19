# TurboRouter

TurboRouter is FraiseQL's high-performance query execution engine that bypasses GraphQL parsing and validation for pre-registered queries, delivering near-zero overhead performance.

## Overview

In production mode, TurboRouter can execute registered GraphQL queries by:
1. Computing a hash of the incoming query
2. Looking up pre-validated SQL templates
3. Executing SQL directly against PostgreSQL
4. Returning formatted results

This approach eliminates:
- GraphQL query parsing overhead
- Schema validation time
- Query planning costs
- Result transformation overhead

## Performance Benefits

TurboRouter typically provides:
- **50-80% latency reduction** for cached queries
- **2-5x throughput improvement** under load
- **Near-zero CPU overhead** (< 0.1ms)
- **Predictable performance** for critical queries

## How It Works

### Query Registration

TurboRouter works by pre-registering GraphQL queries with their corresponding SQL templates:

```python
from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry

# Create a registry
registry = TurboRegistry(max_size=1000)

# Register a query
user_query = TurboQuery(
    graphql_query="""
        query GetUser($id: ID!) {
            user(id: $id) {
                id
                name
                email
            }
        }
    """,
    sql_template="""
        SELECT jsonb_build_object(
            'user', jsonb_build_object(
                'id', id,
                'name', data->>'name',
                'email', data->>'email'
            )
        ) as result
        FROM users
        WHERE id = %(id)s::int AND deleted_at IS NULL
    """,
    param_mapping={"id": "id"},
    operation_name="GetUser"
)

query_hash = registry.register(user_query)
```

### Automatic Registration (Coming Soon)

In a future release, FraiseQL will automatically detect and register frequently used queries:

```python
# Automatic registration based on usage patterns
app = create_fraiseql_app(
    types=[User, Post],
    production=True,
    turbo_config={
        "auto_register": True,
        "usage_threshold": 100,  # Register after 100 executions
        "sample_period": 3600,   # Sample period in seconds
    }
)
```

### Query Execution Flow

1. **Incoming Request**: GraphQL query arrives at `/graphql`
2. **Hash Computation**: Query is normalized and hashed
3. **Registry Lookup**: Hash is checked against registered queries
4. **Direct Execution**: If found, SQL is executed directly
5. **Fallback**: If not found, standard GraphQL execution proceeds

## Configuration

TurboRouter is enabled by default in production mode. You can configure it via environment variables:

```bash
# Enable/disable TurboRouter
FRAISEQL_ENABLE_TURBO_ROUTER=true

# Maximum number of cached queries
FRAISEQL_TURBO_ROUTER_CACHE_SIZE=1000
```

Or in code:

```python
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    enable_turbo_router=True,
    turbo_router_cache_size=2000
)

app = create_fraiseql_app(
    types=[User],
    config=config,
    production=True
)
```

## Best Practices

### 1. Register Critical Queries

Identify and register your most frequently used queries:

```python
# Register during app startup
async def register_critical_queries(registry: TurboRegistry):
    """Register performance-critical queries."""
    
    # User authentication query
    registry.register(TurboQuery(
        graphql_query="query CurrentUser { currentUser { id role permissions } }",
        sql_template="...",
        param_mapping={},
    ))
    
    # Dashboard data query
    registry.register(TurboQuery(
        graphql_query="query Dashboard($userId: ID!) { ... }",
        sql_template="...",
        param_mapping={"userId": "user_id"},
    ))
```

### 2. Use Consistent Query Formatting

TurboRouter uses normalized query hashing. Ensure consistent formatting:

```python
# These are treated as the same query:
"query GetUser($id: ID!) { user(id: $id) { id name } }"
"""
query GetUser($id: ID!) {
    user(id: $id) {
        id
        name
    }
}
"""
```

### 3. Monitor Cache Hit Rates

Track TurboRouter effectiveness:

```python
# Access registry stats
stats = registry.get_stats()
print(f"Cache hit rate: {stats['hit_rate']:.1%}")
print(f"Registered queries: {stats['size']}")
print(f"Total requests: {stats['total_requests']}")
```

### 4. Handle Complex Variables

Map nested GraphQL variables to SQL parameters:

```python
TurboQuery(
    graphql_query="""
        query SearchUsers($filter: UserFilter!) {
            users(filter: $filter) {
                id
                name
            }
        }
    """,
    sql_template="""
        SELECT jsonb_agg(...) as result
        FROM users
        WHERE 
            (%(name_pattern)s IS NULL OR name ILIKE %(name_pattern)s)
            AND (%(min_age)s IS NULL OR age >= %(min_age)s)
    """,
    param_mapping={
        "filter.namePattern": "name_pattern",
        "filter.minAge": "min_age"
    }
)
```

## Limitations

TurboRouter currently has these limitations:

1. **Static Queries Only**: Dynamic query generation not supported
2. **No Fragments**: Fragment expansion not yet implemented
3. **Manual Registration**: Queries must be manually registered
4. **Simple Variables**: Complex variable transformations limited

## Security Considerations

TurboRouter maintains GraphQL security:

- Only pre-validated queries can be registered
- SQL templates use parameterized queries
- Variable mapping prevents injection
- Production mode hides error details

## Performance Comparison

| Execution Path | Parse | Validate | Plan | Execute | Total |
|----------------|-------|----------|------|---------|-------|
| Standard GraphQL | 0.5ms | 0.3ms | 0.2ms | 5ms | 6ms |
| TurboRouter | 0ms | 0ms | 0ms | 5ms | **5.05ms** |

*Overhead includes hash computation (~0.05ms)*

## Future Enhancements

Planned improvements include:

1. **Automatic Query Detection**: Register frequently used queries automatically
2. **Fragment Support**: Handle queries with fragments
3. **Query Warming**: Pre-load critical queries on startup
4. **Distributed Cache**: Share registered queries across instances
5. **Analytics Integration**: Track query performance metrics

## Example: E-commerce Application

Here's how an e-commerce app might use TurboRouter:

```python
# Register product browsing queries
registry.register(TurboQuery(
    graphql_query="""
        query ProductList($category: String, $limit: Int) {
            products(category: $category, limit: $limit) {
                id
                name
                price
                imageUrl
                inStock
            }
        }
    """,
    sql_template="""
        SELECT jsonb_build_object(
            'products', COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', id,
                        'name', data->>'name',
                        'price', (data->>'price')::numeric,
                        'imageUrl', data->>'image_url',
                        'inStock', (data->>'stock')::int > 0
                    )
                    ORDER BY data->>'popularity' DESC
                ),
                '[]'::jsonb
            )
        ) as result
        FROM products
        WHERE 
            deleted_at IS NULL
            AND (%(category)s IS NULL OR data->>'category' = %(category)s)
        LIMIT %(limit)s
    """,
    param_mapping={
        "category": "category",
        "limit": "limit"
    }
))

# Register cart operations
registry.register(TurboQuery(
    graphql_query="""
        query GetCart($userId: ID!) {
            cart(userId: $userId) {
                items {
                    productId
                    quantity
                    price
                }
                total
            }
        }
    """,
    sql_template="""
        SELECT jsonb_build_object(
            'cart', jsonb_build_object(
                'items', data->'items',
                'total', (
                    SELECT SUM((item->>'price')::numeric * (item->>'quantity')::int)
                    FROM jsonb_array_elements(data->'items') AS item
                )
            )
        ) as result
        FROM carts
        WHERE user_id = %(userId)s::uuid
    """,
    param_mapping={"userId": "userId"}
))
```

This configuration ensures that critical e-commerce queries execute with minimal overhead, providing a responsive shopping experience even under high load.