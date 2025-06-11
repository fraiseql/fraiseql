# Performance Optimization

FraiseQL is designed for high performance, but there are several strategies to optimize your API for production workloads.

## Production Mode

Always enable production mode for deployed applications:

```python
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=True,  # Critical for performance
    auto_camel_case=True
)
```

Production mode provides:
- **Bypassed GraphQL validation** - Direct SQL execution
- **No schema introspection** - Reduced overhead and better security
- **Query caching** - Pre-compiled queries for repeat requests
- **Optimized error handling** - Minimal error details for security

## Database Optimizations

### 1. Use Materialized Views for Expensive Queries

For complex aggregations or frequently accessed data:

```sql
-- User statistics materialized view
CREATE MATERIALIZED VIEW v_user_stats AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'post_count', COUNT(DISTINCT p.id),
        'comment_count', COUNT(DISTINCT c.id),
        'total_views', COALESCE(SUM(p.view_count), 0),
        'last_active', MAX(GREATEST(
            p.created_at,
            c.created_at,
            u.updated_at
        ))
    ) as data
FROM tb_users u
LEFT JOIN tb_posts p ON p.author_id = u.id
LEFT JOIN tb_comments c ON c.author_id = u.id
GROUP BY u.id, u.name, u.updated_at;

-- Create unique index for concurrent refresh
CREATE UNIQUE INDEX idx_user_stats_id ON v_user_stats(id);

-- Refresh every hour
SELECT cron.schedule(
    'refresh-user-stats',
    '0 * * * *',
    'REFRESH MATERIALIZED VIEW CONCURRENTLY v_user_stats;'
);
```

### 2. Optimize JSONB Indexes

Create targeted indexes for your query patterns:

```sql
-- GIN index for general JSONB queries
CREATE INDEX idx_posts_data_gin ON tb_posts USING GIN (data);

-- Expression indexes for specific fields
CREATE INDEX idx_posts_published ON tb_posts ((data->>'is_published'));
CREATE INDEX idx_posts_author_id ON tb_posts ((data->>'author_id'));
CREATE INDEX idx_posts_tags ON tb_posts USING GIN ((data->'tags'));

-- Composite indexes for common filter combinations
CREATE INDEX idx_posts_published_created ON tb_posts (
    (data->>'is_published'),
    (data->>'created_at')
) WHERE (data->>'is_published')::boolean = true;
```

### 3. Projection Tables for Ultra-High Performance

For maximum performance, use regular tables that store pre-computed JSON:

```sql
-- Create projection table
CREATE TABLE v_posts_hot (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Trigger to maintain projection
CREATE OR REPLACE FUNCTION update_posts_projection()
RETURNS TRIGGER AS $$
BEGIN
    -- Compose data from multiple sources
    WITH post_data AS (
        SELECT
            NEW.id,
            jsonb_build_object(
                'id', NEW.id,
                'title', NEW.title,
                'author', (
                    SELECT jsonb_build_object('id', u.id, 'name', u.name)
                    FROM tb_users u WHERE u.id = NEW.author_id
                ),
                'comment_count', (
                    SELECT COUNT(*) FROM tb_comments
                    WHERE post_id = NEW.id
                ),
                'is_published', NEW.is_published,
                'created_at', NEW.created_at
            ) as data
    )
    INSERT INTO v_posts_hot (id, data)
    SELECT id, data FROM post_data
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data, updated_at = CURRENT_TIMESTAMP;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER posts_projection_trigger
AFTER INSERT OR UPDATE ON tb_posts
FOR EACH ROW EXECUTE FUNCTION update_posts_projection();
```

## Connection Pooling

Configure appropriate connection pool settings:

```python
import asyncpg

async def create_pool():
    return await asyncpg.create_pool(
        "postgresql://localhost/mydb",
        min_size=5,        # Minimum connections
        max_size=20,       # Maximum connections
        max_queries=50000, # Rotate connections
        max_inactive_connection_lifetime=300,  # 5 minutes
        command_timeout=60,  # Query timeout
    )

app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=True,
    connection_pool_factory=create_pool
)
```

## Query Optimization

### 1. Limit Result Sets

Always use reasonable limits and pagination:

```python
@fraiseql.field
async def posts(
    self,
    info: fraiseql.Info,
    limit: int = 20,  # Default limit
    offset: int = 0
) -> list[Post]:
    """Get posts with pagination."""
    # Enforce maximum limit
    limit = min(limit, 100)

    repo = CQRSRepository(info.context["db"])
    posts_data = await repo.query(
        "v_posts",
        limit=limit,
        offset=offset
    )
    return [Post.from_dict(data) for data in posts_data]
```

### 2. Use Selective Field Queries

Leverage FraiseQL's automatic field selection:

```python
# FraiseQL automatically generates:
# SELECT data->>'title', data->>'excerpt' FROM v_posts
# Instead of: SELECT data FROM v_posts

# This saves bandwidth and processing time
```

### 3. Optimize Filters

Use indexed fields for filtering:

```python
@fraiseql.input
class PostFilters:
    # These should have database indexes
    is_published: Optional[bool] = None
    author_id: Optional[UUID] = None
    created_after: Optional[datetime] = None

    # Avoid expensive operations
    # title_contains: Optional[str] = None  # Full-text search is better
```

## Caching Strategies

### 1. Query Result Caching

Cache expensive query results:

```python
import redis
from functools import wraps

redis_client = redis.Redis(host='localhost', port=6379, db=0)

def cache_result(ttl: int = 300):
    """Cache query results for TTL seconds."""
    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            # Generate cache key
            cache_key = f"{func.__name__}:{hash(str(args) + str(kwargs))}"

            # Try cache first
            cached = redis_client.get(cache_key)
            if cached:
                return json.loads(cached)

            # Execute query
            result = await func(*args, **kwargs)

            # Cache result
            redis_client.setex(
                cache_key,
                ttl,
                json.dumps(result, default=str)
            )

            return result
        return wrapper
    return decorator

@fraiseql.field
@cache_result(ttl=600)  # Cache for 10 minutes
async def popular_posts(self, info: fraiseql.Info) -> list[Post]:
    """Get popular posts (cached)."""
    # Expensive query implementation
    pass
```

### 2. Application-Level Caching

Use Redis or Memcached for application caching:

```python
from fraiseql.cache import RedisCache

app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=True,
    cache=RedisCache(
        url="redis://localhost:6379",
        default_ttl=300,  # 5 minutes
        key_prefix="fraiseql:"
    )
)
```

## Monitoring and Profiling

### 1. Enable Query Logging

Monitor slow queries:

```python
import logging

# Log all SQL queries
logging.getLogger('fraiseql.sql').setLevel(logging.INFO)

# Log slow queries only
logging.getLogger('fraiseql.sql.slow').setLevel(logging.WARNING)
```

### 2. Add Performance Metrics

Track query performance:

```python
import time
from fraiseql.middleware import PerformanceMiddleware

@app.middleware("http")
async def performance_middleware(request, call_next):
    start_time = time.time()
    response = await call_next(request)
    process_time = time.time() - start_time

    response.headers["X-Process-Time"] = str(process_time)

    # Log slow requests
    if process_time > 1.0:  # Slower than 1 second
        logging.warning(f"Slow request: {request.url} took {process_time:.2f}s")

    return response
```

### 3. Database Query Analysis

Use PostgreSQL's built-in tools:

```sql
-- Enable query logging
ALTER SYSTEM SET log_statement = 'all';
ALTER SYSTEM SET log_duration = on;
ALTER SYSTEM SET log_min_duration_statement = 1000;  -- Log queries > 1s

-- Analyze slow queries
SELECT query, calls, total_time, mean_time
FROM pg_stat_statements
ORDER BY total_time DESC
LIMIT 10;

-- Check index usage
SELECT schemaname, tablename, attname, n_distinct, correlation
FROM pg_stats
WHERE tablename = 'tb_posts';
```

## Production Configuration

### Environment Variables

```bash
# Database
DATABASE_URL=postgresql://user:pass@host:5432/db
DATABASE_POOL_MIN_SIZE=5
DATABASE_POOL_MAX_SIZE=20

# Performance
FRAISEQL_PRODUCTION=true
FRAISEQL_QUERY_CACHE_SIZE=1000
FRAISEQL_AUTO_CAMEL_CASE=true

# Caching
REDIS_URL=redis://localhost:6379
CACHE_DEFAULT_TTL=300

# Monitoring
LOG_LEVEL=INFO
SLOW_QUERY_THRESHOLD=1.0
```

### Production Checklist

- [ ] **Production mode enabled**
- [ ] **Connection pooling configured**
- [ ] **Database indexes created**
- [ ] **Materialized views for heavy queries**
- [ ] **Query result caching implemented**
- [ ] **Rate limiting enabled**
- [ ] **Monitoring and logging setup**
- [ ] **Security headers configured**
- [ ] **CORS properly configured**
- [ ] **Load testing completed**

## Load Testing

Test your API under realistic load:

```python
# locustfile.py
from locust import HttpUser, task, between

class GraphQLUser(HttpUser):
    wait_time = between(1, 3)

    @task(3)
    def query_posts(self):
        """Query posts - most common operation."""
        self.client.post("/graphql", json={
            "query": """
                query {
                    posts(limit: 20) {
                        id
                        title
                        excerpt
                        createdAt
                    }
                }
            """
        })

    @task(1)
    def query_user(self):
        """Query user details - less common."""
        self.client.post("/graphql", json={
            "query": """
                query {
                    user(id: "123") {
                        id
                        name
                        email
                    }
                }
            """
        })

# Run: locust -f locustfile.py --host=http://localhost:8000
```

## Performance Benchmarks

Typical FraiseQL performance (on modern hardware):

- **Simple queries**: 10,000+ requests/second
- **Complex queries with joins**: 1,000+ requests/second
- **Materialized view queries**: 50,000+ requests/second
- **Production mode speedup**: 3-5x faster than development mode

These optimizations will ensure your FraiseQL API performs excellently under production load.
