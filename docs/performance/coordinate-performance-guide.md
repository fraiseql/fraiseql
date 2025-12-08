# Coordinate Performance Guide

This guide covers performance optimizations for coordinate fields in FraiseQL applications.

## Database Indexes

### GiST Indexes for Spatial Queries

Coordinate fields should use GiST indexes for optimal spatial query performance:

```sql
-- Create GiST index on coordinate column
CREATE INDEX CONCURRENTLY idx_table_coordinates_gist
ON your_table
USING GIST ((coordinates::point));
```

**Benefits:**
- Fast distance queries: `ST_DWithin(coordinates::point, center_point, radius)`
- Spatial containment queries
- Nearest neighbor searches with `<->` operator

### When to Use GiST vs B-tree

- **Use GiST** for spatial operations (distance, containment, nearest neighbor)
- **Use B-tree** only for exact coordinate equality (rare use case)
- **Use both** if you need both spatial and exact equality queries

## Query Optimization

### Distance Queries

For distance-based filtering, use `ST_DWithin` with proper indexing:

```sql
-- Fast with GiST index
SELECT * FROM locations
WHERE ST_DWithin(coordinates::point, ST_Point(lng, lat)::point, radius_meters);
```

### Nearest Neighbor Queries

Use the distance operator with `ORDER BY` and `LIMIT`:

```sql
-- Find 10 nearest locations
SELECT *, (coordinates::point <-> ST_Point(lng, lat)::point) as distance
FROM locations
ORDER BY coordinates::point <-> ST_Point(lng, lat)::point
LIMIT 10;
```

## Application-Level Optimizations

### Coordinate Validation Caching

Coordinate validation can be expensive for bulk operations. Consider caching validation results:

```python
from functools import lru_cache

@lru_cache(maxsize=1000)
def validate_coordinate_cached(lat: float, lng: float) -> tuple[float, float]:
    # Your validation logic here
    return lat, lng
```

### Batch Coordinate Operations

For bulk inserts/updates, batch coordinate validations:

```python
def validate_coordinates_batch(coordinates: list[tuple[float, float]]) -> list[tuple[float, float]]:
    validated = []
    for coord in coordinates:
        # Validate each coordinate
        validated.append(validate_coordinate(*coord))
    return validated
```

## PostgreSQL Configuration

### PostGIS Tuning

For high-performance spatial operations, ensure PostGIS is properly configured:

```sql
-- Check PostGIS version
SELECT PostGIS_Version();

-- Enable spatial indexes
SET enable_seqscan = off;  -- Force index usage for testing
```

### Memory Configuration

Increase work memory for complex spatial queries:

```sql
SET work_mem = '256MB';  -- Increase for large spatial datasets
```

## Monitoring Performance

### Query Analysis

Use `EXPLAIN ANALYZE` to verify index usage:

```sql
EXPLAIN ANALYZE
SELECT * FROM locations
WHERE ST_DWithin(coordinates::point, ST_Point(-122.4, 37.8)::point, 1000);
```

**Look for:**
- "Index Scan" instead of "Seq Scan"
- GiST index usage
- Reasonable execution time

### Index Usage Statistics

Monitor index effectiveness:

```sql
-- Check index usage
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE indexname LIKE '%coordinates%';
```

## Migration Strategy

When adding coordinates to existing tables:

1. **Create GiST index concurrently** (doesn't block writes):
   ```sql
   CREATE INDEX CONCURRENTLY idx_table_coordinates_gist
   ON your_table USING GIST ((coordinates::point));
   ```

2. **Monitor performance** before and after index creation

3. **Drop unused indexes** if they exist

## Common Performance Issues

### Sequential Scans

**Problem:** Queries not using spatial indexes
**Solution:** Ensure GiST indexes exist and queries use `ST_DWithin`

### Slow Bulk Inserts

**Problem:** Index maintenance during bulk loads
**Solution:** Drop indexes during bulk insert, recreate afterward

### Memory Issues

**Problem:** Out of memory on large spatial datasets
**Solution:** Increase `work_mem`, use pagination, or optimize queries

## Benchmarking

Use the provided coordinate benchmarks to measure performance:

```bash
# Run coordinate-specific benchmarks
uv run pytest benchmarks/ -k coordinate

# Profile spatial queries
EXPLAIN ANALYZE SELECT * FROM locations WHERE ST_DWithin(...);
```
