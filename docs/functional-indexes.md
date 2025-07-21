# Functional Indexes for FraiseQL

This document explains how to create and optimize PostgreSQL indexes for FraiseQL's JSONB-based query patterns.

## Overview

FraiseQL stores all data in JSONB columns within PostgreSQL views. This provides flexibility but requires careful indexing to maintain query performance. PostgreSQL's functional indexes are key to achieving this.

## Basic Concepts

### JSONB Structure

FraiseQL views follow this pattern:
```sql
CREATE VIEW user_view AS
SELECT
    id,                    -- Used for filtering
    tenant_id,             -- For multi-tenancy
    created_at,            -- For ordering
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'status', status
    ) as data              -- All fields in JSONB
FROM users;
```

### Query Patterns

FraiseQL generates SQL queries like:
```sql
-- Filtering by JSONB field
SELECT data FROM user_view
WHERE (data ->> 'email') = 'user@example.com';

-- Numeric comparisons
SELECT data FROM user_view
WHERE (data ->> 'age')::numeric > 21;

-- Pattern matching
SELECT data FROM user_view
WHERE (data ->> 'name') ILIKE '%john%';
```

## Creating Functional Indexes

### 1. Text Field Indexes

For exact matches on text fields:
```sql
CREATE INDEX idx_user_email ON users
    ((data ->> 'email'));

-- For case-insensitive searches
CREATE INDEX idx_user_email_lower ON users
    (lower(data ->> 'email'));
```

### 2. Numeric Field Indexes

For numeric comparisons:
```sql
CREATE INDEX idx_product_price ON products
    (((data ->> 'price')::numeric));

-- For integer fields
CREATE INDEX idx_user_age ON users
    (((data ->> 'age')::int));
```

### 3. Boolean Field Indexes

For boolean filters:
```sql
CREATE INDEX idx_user_active ON users
    (((data ->> 'active')::boolean));
```

### 4. Timestamp Field Indexes

For date/time queries:
```sql
CREATE INDEX idx_order_created ON orders
    (((data ->> 'created_at')::timestamptz));
```

### 5. Array Field Indexes

For array contains operations:
```sql
-- GIN index for array contains
CREATE INDEX idx_user_tags ON users USING gin
    ((data -> 'tags'));

-- For array length queries
CREATE INDEX idx_user_tags_length ON users
    ((jsonb_array_length(data -> 'tags')));
```

### 6. Nested Object Indexes

For nested JSONB queries:
```sql
-- Index on nested field
CREATE INDEX idx_user_address_city ON users
    ((data -> 'address' ->> 'city'));

-- Composite index on multiple nested fields
CREATE INDEX idx_user_location ON users
    ((data -> 'address' ->> 'country'),
     (data -> 'address' ->> 'city'));
```

## Optimization Strategies

### 1. Multi-Column Indexes

Combine frequently filtered fields:
```sql
CREATE INDEX idx_user_status_created ON users
    ((data ->> 'status'), created_at DESC);
```

### 2. Partial Indexes

Index only relevant rows:
```sql
-- Index only active users
CREATE INDEX idx_active_user_email ON users
    ((data ->> 'email'))
    WHERE (data ->> 'status') = 'active';

-- Index only recent records
CREATE INDEX idx_recent_orders ON orders
    ((data ->> 'customer_id'))
    WHERE created_at > CURRENT_DATE - INTERVAL '30 days';
```

### 3. Expression Indexes

For complex expressions:
```sql
-- Full name search
CREATE INDEX idx_user_full_name ON users
    ((data ->> 'first_name' || ' ' || data ->> 'last_name'));

-- Computed values
CREATE INDEX idx_order_total_with_tax ON orders
    ((((data ->> 'total')::numeric * 1.08)));
```

### 4. Text Search Indexes

For full-text search:
```sql
-- GIN index for text search
CREATE INDEX idx_product_search ON products USING gin
    (to_tsvector('english',
        coalesce(data ->> 'name', '') || ' ' ||
        coalesce(data ->> 'description', '')
    ));
```

## Performance Analysis

### Query Planning

Use EXPLAIN ANALYZE to verify index usage:
```sql
EXPLAIN (ANALYZE, BUFFERS)
SELECT data FROM user_view
WHERE (data ->> 'email') = 'user@example.com';
```

### Index Statistics

Monitor index usage:
```sql
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;
```

### Identifying Missing Indexes

Find slow queries without indexes:
```sql
SELECT
    query,
    calls,
    total_time,
    mean_time,
    rows
FROM pg_stat_statements
WHERE query LIKE '%data ->>%'
ORDER BY mean_time DESC
LIMIT 20;
```

## Best Practices

### 1. Index Naming Convention

Use descriptive names:
```
idx_<table>_<field>_<type>
idx_user_email_text
idx_product_price_numeric
idx_order_status_created_composite
```

### 2. Regular Maintenance

```sql
-- Rebuild indexes periodically
REINDEX INDEX idx_user_email;

-- Update statistics
ANALYZE users;

-- Monitor bloat
SELECT
    schemaname,
    tablename,
    indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes
ORDER BY pg_relation_size(indexrelid) DESC;
```

### 3. Consider Trade-offs

- **Write Performance**: Each index slows down INSERT/UPDATE operations
- **Storage**: Indexes consume disk space
- **Maintenance**: More indexes mean more maintenance overhead

### 4. Start Simple

1. Begin with indexes on primary filter fields
2. Add indexes based on actual query patterns
3. Monitor and adjust based on performance metrics

## Common Patterns

### Multi-Tenant Queries

```sql
-- Composite index for tenant isolation
CREATE INDEX idx_user_tenant_email ON users
    (tenant_id, (data ->> 'email'));
```

### Time-Series Data

```sql
-- Optimize for recent data access
CREATE INDEX idx_event_recent ON events
    (created_at DESC)
    WHERE created_at > CURRENT_DATE - INTERVAL '7 days';
```

### Geospatial Queries

```sql
-- PostGIS functional index
CREATE INDEX idx_location_point ON locations USING gist
    (ST_MakePoint(
        (data ->> 'longitude')::float,
        (data ->> 'latitude')::float
    ));
```

## Monitoring and Alerts

Set up monitoring for:
1. Queries without index scans
2. Index bloat > 50%
3. Unused indexes (idx_scan = 0)
4. Slow queries (> 100ms)

## Conclusion

Functional indexes are essential for FraiseQL performance. Start with the most common query patterns and gradually optimize based on real-world usage. Regular monitoring and maintenance ensure continued performance as your application grows.
