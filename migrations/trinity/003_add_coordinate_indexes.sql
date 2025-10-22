-- Coordinate Indexes Migration
--
-- Adds performance indexes for coordinate fields to enable fast spatial queries.
-- This migration should be run after adding coordinate fields to any table.
--
-- Coordinate fields use PostgreSQL POINT type, so we need GiST indexes for:
--   - Distance queries (ST_DWithin)
--   - Spatial containment queries
--   - Nearest neighbor searches
--
-- Usage: Replace 'table_name' and 'coordinate_column' with actual values

BEGIN;

-- ============================================================================
-- STEP 1: Add GiST index for spatial queries
-- ============================================================================

-- Create GiST index on coordinate column for spatial operations
-- This enables fast distance calculations and spatial queries
CREATE INDEX CONCURRENTLY idx_table_name_coordinate_column_gist
ON table_name
USING GIST ((coordinate_column::point));

-- ============================================================================
-- STEP 2: Add regular B-tree index for exact equality (optional)
-- ============================================================================

-- For exact coordinate matches, a regular index may be faster
-- Only add this if you frequently do exact coordinate equality queries
-- CREATE INDEX CONCURRENTLY idx_table_name_coordinate_column_exact
-- ON table_name (coordinate_column);

-- ============================================================================
-- STEP 3: Add comments for documentation
-- ============================================================================

COMMENT ON INDEX idx_table_name_coordinate_column_gist IS
    'GiST index for spatial queries on coordinate field (distance, containment, nearest neighbor)';

-- ============================================================================
-- STEP 4: Analyze table for query planner
-- ============================================================================

-- Update table statistics for the query planner
ANALYZE table_name;

COMMIT;

-- ============================================================================
-- Verification Queries (run after migration)
-- ============================================================================

-- Check index was created
-- SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'table_name';

-- Test spatial query performance (should use GiST index)
-- EXPLAIN ANALYZE
-- SELECT * FROM table_name
-- WHERE ST_DWithin(coordinate_column::point, ST_Point(-122.4194, 37.7749)::point, 1000);

-- Example spatial queries that benefit from GiST index:
-- 1. Distance queries: ST_DWithin(coordinate_column::point, center_point, radius)
-- 2. Containment: coordinate_column::point <@ bounding_box
-- 3. Nearest neighbors: coordinate_column::point <-> target_point (with ORDER BY and LIMIT)
