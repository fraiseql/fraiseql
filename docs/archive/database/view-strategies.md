# View Strategies for FraiseQL

**Time to Complete:** 15 minutes
**Prerequisites:** Understanding of [Table Naming Conventions](./table-naming-conventions.md)

## Overview

FraiseQL provides multiple view strategies to optimize read performance for different use cases. This guide helps you choose the right approach for your application.

---

## View Types Comparison

| Strategy | Performance | Complexity | Freshness | Best For |
|----------|------------|-------------|------------|
| **Standard Views (`v_*`)** | 5-10ms | Low | Always live | Small datasets (<10k rows) |
| **Table Views (`tv_*`)** | 0.05-0.5ms | Medium | Near real-time | Production GraphQL APIs |
| **Materialized Views (`mv_*`)** | 0.1-0.5ms | High | Stale (5-60 min) | Analytics dashboards |

---

## Strategy 1: Standard Views (`v_*`)

### When to Use
- Small datasets (<10,000 rows)
- Development/prototyping
- When absolute freshness is required
- Storage constraints (no extra space for table views)

### Implementation
```sql
-- Standard SQL view
CREATE VIEW v_user AS
SELECT
    u.id,
    u.first_name,
    u.last_name,
    u.email,
    u.created_at,
    -- Embedded posts as JSON
    (
        SELECT json_agg(
            json_build_object(
                'id', p.id,
                'title', p.title,
                'created_at', p.created_at
            )
            ORDER BY p.created_at DESC
        )
        FROM tb_post p
        WHERE p.user_id = u.id
        LIMIT 10
    ) as posts_json
FROM tb_user u;
```

### Performance Characteristics
- **Read Time**: 5-10ms (JOIN + subquery on every read)
- **Write Time**: 0.5ms (no sync needed)
- **Storage**: 1x (no extra storage)
- **Freshness**: Always live

### GraphQL Integration
```python
import fraiseql

@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    first_name: str
    last_name: str
    email: str
    posts_json: list[dict]  # JSON, not transformed

@fraiseql.query
async def user(info, id: UUID) -> User:
    repo = info.context["db"]
    return await repo.find_one("v_user", id=id)
```

---

## Strategy 2: Table Views (`tv_*`) - **Recommended for Production**

### When to Use
- Production GraphQL APIs (recommended)
- Large datasets (>100,000 rows)
- Read-heavy workloads (10:1+ read:write ratio)
- Sub-millisecond response times required

### Implementation
```sql
-- Table view (regular table with explicit sync)
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sync function (explicit)
CREATE FUNCTION fn_sync_tv_user(p_id UUID) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user (id, data)
    SELECT
        u.id,
        jsonb_build_object(
            'id', u.id,
            'first_name', u.first_name,
            'last_name', u.last_name,
            'email', u.email,
            'created_at', u.created_at,
            'user_posts', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', p.id,
                        'title', p.title,
                        'content', p.content,
                        'created_at', p.created_at
                    )
                    ORDER BY p.created_at DESC
                )
                FROM tb_post p
                WHERE p.user_id = u.id
                LIMIT 10
            )
        )
    FROM tb_user u
    WHERE u.id = p_id
    ON CONFLICT (id) DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Triggers for automatic sync
CREATE TRIGGER trg_sync_tv_user
AFTER INSERT OR UPDATE OR DELETE ON tb_user
FOR EACH ROW EXECUTE FUNCTION trg_sync_tv_user();

CREATE TRIGGER trg_sync_tv_user_on_post
AFTER INSERT OR UPDATE OR DELETE ON tb_post
FOR EACH ROW EXECUTE FUNCTION trg_sync_tv_user_on_post();
```

### Performance Characteristics
- **Read Time**: 0.05-0.5ms (simple indexed lookup)
- **Write Time**: 1-2ms (sync triggers)
- **Storage**: 1.5-2x (denormalized data)
- **Freshness**: Near real-time (sync on write)

### GraphQL Integration
```python
import fraiseql

@fraiseql.type(sql_source="tv_user", jsonb_column="data")
class User:
    id: UUID
    first_name: str
    last_name: str
    email: str
    user_posts: list[Post]  # Transformed from JSONB

@fraiseql.query
async def user(info, id: UUID) -> User:
    repo = info.context["db"]
    return await repo.find_one("tv_user", id=id)
```

---

## Strategy 3: Materialized Views (`mv_*`)

### When to Use
- Complex aggregations (GROUP BY, COUNT, SUM)
- Analytics dashboards
- Acceptable data staleness (5-60 minutes)
- Read-heavy analytical queries

### Implementation
```sql
-- Materialized view for analytics
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT
    u.id,
    u.first_name,
    u.last_name,
    COUNT(p.id) as post_count,
    MAX(p.created_at) as last_post_at,
    AVG(
        EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - p.created_at)) / 86400
    ) as avg_days_between_posts
FROM tb_user u
LEFT JOIN tb_post p ON p.user_id = u.id
GROUP BY u.id, u.first_name, u.last_name;

-- Indexes for fast queries
CREATE INDEX idx_mv_user_stats_post_count ON mv_user_stats(post_count);
CREATE INDEX idx_mv_user_stats_last_post ON mv_user_stats(last_post_at);

-- Refresh function
CREATE FUNCTION fn_refresh_user_stats() RETURNS VOID AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats;
END;
$$ LANGUAGE plpgsql;
```

### Refresh Strategy
```sql
-- Cron job to refresh every 30 minutes
SELECT cron.schedule(
    '0,30 * * * *',  -- Every 30 minutes
    $$SELECT fn_refresh_user_stats()$$
);
```

### Performance Characteristics
- **Read Time**: 0.1-0.5ms (pre-computed)
- **Write Time**: N/A (batch refresh)
- **Storage**: 1.2-1.5x (aggregated data)
- **Freshness**: Stale (until next refresh)

---

## Decision Guide

### Use `v_*` When:
```yaml
Conditions:
  dataset_size: "< 10k"
  traffic_pattern: "Low to moderate"
  freshness_requirement: "Absolute"
  storage_constraints: true
  development_phase: true
```

### Use `tv_*` When:
```yaml
Conditions:
  dataset_size: "> 100k"
  traffic_pattern: "High"
  freshness_requirement: "Near real-time"
  performance_requirement: "Sub-millisecond"
  production_environment: true
```

### Use `mv_*` When:
```yaml
Conditions:
  query_type: "Analytics/Aggregation"
  complexity: "High (GROUP BY, multiple JOINs)"
  freshness_tolerance: "5-60 minutes"
  dashboard_use_case: true
```

---

## Migration Path

### From `v_*` to `tv_*`
```sql
-- Step 1: Create table view
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Step 2: Populate from existing view
INSERT INTO tv_user (id, data)
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'first_name', first_name,
        'last_name', last_name,
        'email', email,
        'created_at', created_at
    )
FROM v_user;

-- Step 3: Create sync triggers
-- (See tv_* implementation above)

-- Step 4: Update GraphQL types to use tv_user
-- (See GraphQL integration above)
```

### From Direct Queries to `mv_*`
```sql
-- Step 1: Create materialized view
CREATE MATERIALIZED VIEW mv_dashboard AS
SELECT ...;  -- Your complex query

-- Step 2: Create indexes
CREATE INDEX idx_mv_dashboard_metric ON mv_dashboard(metric);
CREATE INDEX idx_mv_dashboard_date ON mv_dashboard(date);

-- Step 3: Set up refresh schedule
SELECT cron.schedule('0 * * * *', $$REFRESH MATERIALIZED VIEW CONCURRENTLY mv_dashboard$$);
```

---

## Best Practices

### 1. Naming Consistency
```sql
-- ✅ Correct naming
CREATE VIEW v_user AS ...           -- Standard view
CREATE TABLE tv_user AS ...         -- Table view
CREATE MATERIALIZED VIEW mv_stats AS ...  -- Materialized view

-- ❌ Avoid
CREATE VIEW user_view AS ...         -- Inconsistent
CREATE TABLE user_cache AS ...       -- Unclear purpose
```

### 2. Index Strategy
```sql
-- For v_* views: Index underlying tables
CREATE INDEX idx_user_email ON tb_user(email);

-- For tv_* tables: Index the primary key
CREATE INDEX idx_tv_user_id ON tv_user(id);

-- For mv_* views: Index aggregated columns
CREATE INDEX idx_mv_stats_count ON mv_user_stats(post_count);
```

### 3. Monitoring
```sql
-- Check view performance
EXPLAIN ANALYZE SELECT * FROM v_user WHERE id = $1;

-- Check materialized view freshness
SELECT
    pg_size.pretty_size(pg_relation_size('mv_user_stats')),
    pg_stat_get_last_vacuum_time('mv_user_stats'::regclass);
```

---

## Performance Benchmarks

### Dataset: 100k users, 500k posts

| Operation | `v_*` View | `tv_*` Table | `mv_*` Materialized |
|-----------|--------------|---------------|-------------------|
| Single user lookup | 8.2ms | 0.23ms | 0.15ms |
| User with posts (10) | 12.5ms | 0.31ms | 0.18ms |
| Count users by date | 45.3ms | 2.1ms | 0.09ms |
| Storage overhead | 0% | 85% | 42% |
| Sync/Refresh cost | 0ms | 1.8ms per write | 2.3s per refresh |

---

## Next Steps

- [Table Naming Conventions](./table-naming-conventions.md) - Complete naming reference
- [Database Level Caching](./database-level-caching.md) - Caching strategies
- [Migration Guide](./migrations.md) - Migrate between patterns

---

**Recommendation**: Use `tv_*` table views for production GraphQL APIs. They provide the best balance of performance and freshness for most applications.
