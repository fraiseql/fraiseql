# Performance Optimization Guide

Comprehensive guide to optimizing FraiseQL applications for maximum performance in production environments.

## Performance Philosophy

FraiseQL achieves high performance through:
1. **Direct SQL generation** - No ORM overhead
2. **Composable views** - Single queries for complex data
3. **JSONB optimization** - Native PostgreSQL performance
4. **Connection pooling** - Efficient resource usage
5. **Multiple execution modes** - Automatic optimization

## Query Optimization

### Use Composable Views

The most impactful optimization is using composable SQL views to eliminate N+1 queries:

```sql
-- Bad: N+1 queries
-- First query: SELECT * FROM users WHERE id = $1
-- Then N queries: SELECT * FROM posts WHERE author_id = $1

-- Good: Single query with composed view
CREATE VIEW v_user_with_posts AS
SELECT
    u.id,
    u.name,
    u.email,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'posts', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'content', p.content,
                    'created_at', p.created_at
                )
            )
            FROM posts p
            WHERE p.author_id = u.id
            ORDER BY p.created_at DESC
            ), '[]'::jsonb
        )
    ) as data
FROM users u;
```

```python
@fraise_type
class UserWithPosts:
    id: UUID
    name: str
    email: str
    posts: list[Post]

@query
async def get_user_with_posts(info, id: UUID) -> UserWithPosts:
    db = info.context["db"]
    # Single query fetches everything
    return await db.find_one("v_user_with_posts", {"id": id})
```

### Index Optimization

Create appropriate indexes for your views and queries:

```sql
-- Index for primary lookups
CREATE INDEX idx_users_id ON users(id);

-- Index for foreign key relationships
CREATE INDEX idx_posts_author_id ON posts(author_id);

-- Composite index for filtered queries
CREATE INDEX idx_posts_author_created
    ON posts(author_id, created_at DESC);

-- GIN index for JSONB searches
CREATE INDEX idx_users_data_gin ON users USING gin(data);

-- Partial index for common filters
CREATE INDEX idx_posts_published
    ON posts(author_id)
    WHERE status = 'published';
```

### Query Complexity Management

Configure complexity limits to prevent expensive queries:

```python
config = FraiseQLConfig(
    complexity_enabled=True,
    complexity_max_score=1000,
    complexity_max_depth=10,
    complexity_default_list_size=10,
    complexity_field_multipliers={
        "search": 5,  # Search is expensive
        "aggregate": 10,  # Aggregations are very expensive
    }
)
```

## Connection Pool Tuning

### Pool Size Calculation

```python
# Formula: pool_size = (max_connections * 0.8) / number_of_app_instances

# Example for production
config = FraiseQLConfig(
    # PostgreSQL max_connections = 200
    # 4 app instances
    # Pool size = (200 * 0.8) / 4 = 40
    database_pool_size=40,
    database_max_overflow=10,  # 25% overflow
    database_pool_timeout=5,   # Fast failure in production
)
```

### PostgreSQL Configuration

Optimize PostgreSQL for your workload:

```sql
-- postgresql.conf optimizations
shared_buffers = '4GB'  # 25% of RAM
effective_cache_size = '12GB'  # 75% of RAM
work_mem = '16MB'
maintenance_work_mem = '256MB'
random_page_cost = 1.1  # For SSD storage
effective_io_concurrency = 200  # For SSD storage

-- Connection settings
max_connections = 200
max_prepared_transactions = 100

-- Query planner
default_statistics_target = 100
```

## TurboRouter Optimization

TurboRouter works seamlessly with lazy caching to provide maximum performance by bypassing GraphQL parsing entirely.

### Registering Hot Queries

Identify and register frequently used queries:

```python
from fraiseql.fastapi import TurboRegistry, TurboQuery

# In your app startup
registry = TurboRegistry(max_size=1000)

# Register common queries
user_by_id = TurboQuery(
    graphql_query="""
        query GetUser($id: UUID!) {
            getUser(id: $id) {
                id
                name
                email
            }
        }
    """,
    sql_template="""
        SELECT
            id::text,
            name,
            email
        FROM v_user
        WHERE id = %(id)s
    """,
    param_mapping={"id": "id"}
)
registry.register(user_by_id)

# Configure app with registry
app = create_fraiseql_app(
    config=config,
    turbo_registry=registry
)
```

### Auto-Registration Strategy

```python
config = FraiseQLConfig(
    turbo_router_auto_register=True,
    turbo_max_complexity=50,  # Only cache simple queries
    turbo_router_cache_size=500,
    turbo_enable_adaptive_caching=True,
)
```

## JSON Passthrough Optimization

### Enable for Production

```python
config = FraiseQLConfig(
    json_passthrough_enabled=True,
    json_passthrough_in_production=True,
    json_passthrough_cache_nested=True,
    passthrough_complexity_limit=100,
    passthrough_max_depth=5,
)
```

### View Design for Passthrough

Design views with JSONB data columns for optimal passthrough:

```sql
CREATE VIEW v_product_catalog AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'price', p.price,
        'category', c.name,
        'inventory', jsonb_build_object(
            'quantity', i.quantity,
            'warehouse', w.name,
            'last_restocked', i.last_restocked
        ),
        'reviews', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'rating', r.rating,
                    'comment', r.comment,
                    'user', u.name
                )
            )
            FROM reviews r
            JOIN users u ON u.id = r.user_id
            WHERE r.product_id = p.id
        )
    ) as data
FROM products p
JOIN categories c ON c.id = p.category_id
LEFT JOIN inventory i ON i.product_id = p.id
LEFT JOIN warehouses w ON w.id = i.warehouse_id;
```

## Caching Strategies

### Built-in Lazy Caching (Recommended)

FraiseQL's lazy caching system provides database-native caching with automatic invalidation:

```python
# Enable lazy caching for your queries
config = FraiseQLConfig(
    enable_lazy_caching=True,
    cache_version_tracking=True,
)

# Queries automatically use lazy caching
@query
async def get_popular_posts(info, limit: int = 10) -> list[Post]:
    db = info.context["db"]
    # This query is automatically cached in PostgreSQL
    # with version-based invalidation
    return await db.find(
        "v_post",
        order_by="view_count DESC",
        limit=limit
    )
```

Benefits over external caching:
- **No network overhead** - Cache lives in PostgreSQL
- **Automatic invalidation** - Version tracking by bounded contexts
- **Sub-millisecond response** - Direct database access
- **Historical data** - Cache becomes audit trail
- **No Redis/Memcached needed** - Simplified infrastructure

See [Lazy Caching Guide](lazy-caching.md) for complete documentation.

### View Materialization

For expensive aggregations, use materialized views:

```sql
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT
    u.id,
    u.name,
    COUNT(DISTINCT p.id) as post_count,
    COUNT(DISTINCT c.id) as comment_count,
    MAX(p.created_at) as last_post_date,
    SUM(p.view_count) as total_views
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
LEFT JOIN comments c ON c.user_id = u.id
GROUP BY u.id, u.name;

-- Create indexes on materialized view
CREATE INDEX idx_mv_user_stats_id ON mv_user_stats(id);

-- Refresh strategy
CREATE OR REPLACE FUNCTION refresh_user_stats()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats;
END;
$$ LANGUAGE plpgsql;

-- Schedule refresh (using pg_cron or similar)
SELECT cron.schedule(
    'refresh-user-stats',
    '*/15 * * * *',  -- Every 15 minutes
    'SELECT refresh_user_stats()'
);
```

## Monitoring & Profiling

### Enable Performance Metrics

```python
config = FraiseQLConfig(
    include_execution_metadata=True,
    enable_request_logging=True,
)
```

### Query Analysis

Monitor slow queries with PostgreSQL extensions:

```sql
-- Enable pg_stat_statements
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- Find slow queries
SELECT
    query,
    mean_exec_time,
    calls,
    total_exec_time
FROM pg_stat_statements
WHERE query LIKE '%v_%'  -- FraiseQL views
ORDER BY mean_exec_time DESC
LIMIT 20;

-- Analyze specific query plan
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user_with_posts WHERE id = '...';
```

### Application Metrics

```python
from prometheus_client import Counter, Histogram, Gauge

# Custom metrics
query_duration = Histogram(
    'fraiseql_query_duration_seconds',
    'Query execution time',
    ['operation', 'mode']
)

db_pool_size = Gauge(
    'fraiseql_db_pool_connections',
    'Active database connections'
)

cache_hits = Counter(
    'fraiseql_cache_hits_total',
    'Cache hit count',
    ['cache_type']
)
```

## Pagination Optimization

### Cursor-Based Pagination

More efficient than offset for large datasets:

```python
@fraise_input
class CursorPaginationInput:
    first: int = 20
    after: str | None = None
    order_by: str = "created_at"

@query
async def list_posts(
    info,
    pagination: CursorPaginationInput
) -> PaginatedPosts:
    db = info.context["db"]

    # Decode cursor
    where = {}
    if pagination.after:
        cursor_data = decode_cursor(pagination.after)
        where[f"{pagination.order_by}__gt"] = cursor_data

    # Fetch one extra to determine hasNextPage
    posts = await db.find(
        "v_post",
        where=where,
        order_by=pagination.order_by,
        limit=pagination.first + 1
    )

    has_next = len(posts) > pagination.first
    if has_next:
        posts = posts[:-1]

    edges = [
        Edge(
            node=post,
            cursor=encode_cursor(getattr(post, pagination.order_by))
        )
        for post in posts
    ]

    return PaginatedPosts(
        edges=edges,
        page_info=PageInfo(
            has_next_page=has_next,
            end_cursor=edges[-1].cursor if edges else None
        )
    )
```

## Batch Operations

### Bulk Inserts

```python
@mutation
async def bulk_create_users(
    info,
    users: list[CreateUserInput]
) -> BulkCreateResult:
    db = info.context["db"]

    # Use COPY for large batches
    if len(users) > 100:
        async with db.pool.connection() as conn:
            async with conn.cursor() as cur:
                # Use COPY for efficiency
                await cur.copy_records_to_table(
                    'users',
                    records=[(u.name, u.email) for u in users],
                    columns=['name', 'email']
                )
    else:
        # Use batch insert for smaller sets
        values = [
            {"name": u.name, "email": u.email}
            for u in users
        ]
        await db.insert_many("users", values)

    return BulkCreateResult(count=len(users))
```

## Production Checklist

### Database Optimization
- [ ] Create appropriate indexes
- [ ] Build composable views with `v_` prefix
- [ ] Set up materialized views for aggregations
- [ ] Configure PostgreSQL settings
- [ ] Enable pg_stat_statements
- [ ] Set up connection pooling
- [ ] Configure autovacuum properly

### Application Optimization
- [ ] Enable TurboRouter
- [ ] Register hot queries
- [ ] Enable JSON passthrough
- [ ] Configure complexity limits
- [ ] Set up query caching
- [ ] Implement pagination
- [ ] Enable monitoring

### Monitoring Setup
- [ ] Configure Prometheus metrics
- [ ] Set up slow query logging
- [ ] Monitor connection pool usage
- [ ] Track cache hit rates
- [ ] Monitor memory usage
- [ ] Set up alerting

## Performance Targets

*Note: These are estimates for guidance. Actual performance depends on your specific setup.*

### Response Time Targets

| Percentile | Target | Action if Exceeded |
|------------|--------|-------------------|
| p50 | < 10ms | Monitor |
| p95 | < 50ms | Investigate |
| p99 | < 200ms | Optimize |
| p99.9 | < 1s | Alert |

### Throughput Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Queries/sec | > 1000 | Per instance |
| Concurrent connections | < 80% pool | Leave headroom |
| Cache hit ratio | > 80% | For cacheable queries |
| Error rate | < 0.1% | Excluding client errors |

## Common Performance Issues

### Issue: Slow Nested Queries

**Solution**: Use composable views
```sql
-- Instead of nested resolvers, compose in view
CREATE VIEW v_order_complete AS
SELECT
    o.*,
    jsonb_build_object(
        'customer', (SELECT data FROM v_customer WHERE id = o.customer_id),
        'items', (SELECT jsonb_agg(data) FROM v_order_item WHERE order_id = o.id),
        'shipping', (SELECT data FROM v_shipping WHERE order_id = o.id)
    ) as data
FROM orders o;
```

### Issue: Connection Pool Exhaustion

**Solution**: Tune pool settings and query timeout
```python
config = FraiseQLConfig(
    database_pool_size=50,
    database_pool_timeout=5,  # Fail fast
    query_timeout=10,  # Kill long queries
)
```

### Issue: Memory Growth

**Solution**: Limit query complexity and result size
```python
config = FraiseQLConfig(
    complexity_max_score=500,
    max_query_depth=5,
    # Limit default page size
    default_limit=50,
    max_limit=200,
)
```

## Next Steps

- [TurboRouter Configuration](./turbo-router.md) - Maximize performance
- [Database API Patterns](./database-api-patterns.md) - Optimal schema design
- [Monitoring Guide](./monitoring.md) - Production observability
