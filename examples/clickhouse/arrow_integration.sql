-- ClickHouse Arrow Flight Integration Examples
--
-- This file demonstrates how to work with FraiseQL event data in ClickHouse
-- after it has been ingested via the ClickHouseSink from Phase 9.4.

-- ==============================================================================
-- PART 1: Basic Queries on Ingested Events
-- ==============================================================================

-- Count total events by type
SELECT
    event_type,
    count() AS count,
    uniq(entity_id) AS unique_entities
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 7 DAY
GROUP BY event_type
ORDER BY count DESC;

-- Count events by entity type
SELECT
    entity_type,
    count() AS count,
    uniq(entity_id) AS unique_entities,
    uniq(user_id) AS unique_users
FROM fraiseql_events
GROUP BY entity_type
ORDER BY count DESC;

-- Timeline of events (hourly aggregation)
SELECT
    toStartOfHour(timestamp) AS hour,
    event_type,
    count() AS event_count
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 7 DAY
GROUP BY hour, event_type
ORDER BY hour DESC, event_count DESC;

-- ==============================================================================
-- PART 2: Advanced Analytics
-- ==============================================================================

-- Event volume trend (daily)
SELECT
    toDate(timestamp) AS day,
    count() AS total_events,
    uniq(entity_id) AS unique_entities,
    uniq(user_id) AS unique_users,
    countIf(event_type = 'Created') AS created_count,
    countIf(event_type = 'Updated') AS updated_count,
    countIf(event_type = 'Deleted') AS deleted_count
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 30 DAY
GROUP BY day
ORDER BY day DESC;

-- User activity: top users by event count
SELECT
    user_id,
    count() AS event_count,
    uniq(entity_type) AS entity_types_modified,
    uniq(entity_id) AS unique_entities_modified,
    min(timestamp) AS first_event,
    max(timestamp) AS last_event
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 7 DAY
GROUP BY user_id
ORDER BY event_count DESC
LIMIT 100;

-- Entity modification frequency: which entities change most frequently
SELECT
    entity_type,
    entity_id,
    count() AS change_count,
    arrayStringConcat(groupArray(DISTINCT event_type), ',') AS event_types,
    min(timestamp) AS first_change,
    max(timestamp) AS last_change
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 30 DAY
GROUP BY entity_type, entity_id
HAVING change_count > 1
ORDER BY change_count DESC
LIMIT 100;

-- ==============================================================================
-- PART 3: Materialized View Examples
-- ==============================================================================

-- Query from materialized view: hourly statistics
SELECT
    hour,
    event_type,
    entity_type,
    total_count
FROM fraiseql_events_hourly
ORDER BY hour DESC, total_count DESC
LIMIT 50;

-- Query from materialized view: daily organization statistics
SELECT
    day,
    org_id,
    total_events,
    unique_users,
    unique_entities
FROM fraiseql_org_daily
ORDER BY day DESC, total_events DESC;

-- Query from materialized view: event type statistics
SELECT
    event_type,
    hour,
    event_count
FROM fraiseql_event_type_stats
WHERE hour >= now() - INTERVAL 24 HOUR
ORDER BY hour DESC, event_count DESC;

-- ==============================================================================
-- PART 4: JSON Data Analysis
-- ==============================================================================

-- Extract and analyze JSON data from events
-- This demonstrates ClickHouse's JSON extraction capabilities
SELECT
    entity_type,
    JSONExtractString(data, 'status') AS status,
    count() AS count
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 7 DAY
GROUP BY entity_type, status
ORDER BY entity_type, status;

-- ==============================================================================
-- PART 5: Performance Optimization Tips
-- ==============================================================================

-- Use PREWHERE for better performance on large tables
-- (Filtering happens before decompression)
SELECT
    event_type,
    count() AS count
FROM fraiseql_events
PREWHERE timestamp >= now() - INTERVAL 7 DAY
WHERE event_type IN ('Created', 'Updated')
GROUP BY event_type;

-- Use sampling for approximate results on very large datasets
SELECT
    entity_type,
    count() * 100 AS estimated_count  -- extrapolate 1% sample
FROM fraiseql_events
SAMPLE 1/100  -- 1% sample
WHERE timestamp >= now() - INTERVAL 30 DAY
GROUP BY entity_type;

-- ==============================================================================
-- PART 6: Integration with Arrow Flight
-- ==============================================================================

-- Future: When Arrow Flight native table support is available
-- This will allow direct consumption of Arrow streams
--
-- CREATE TABLE fraiseql_flight_stream
-- ENGINE = ArrowFlight(
--     'http://localhost:50051',
--     'fraiseql_events_stream'
-- );
--
-- SELECT * FROM fraiseql_flight_stream LIMIT 1000;

-- For now, use the ClickHouseSink to ingest Arrow RecordBatches
-- See ../../crates/fraiseql-arrow/clickhouse_sink.rs

-- ==============================================================================
-- PART 7: Event Search and Debugging
-- ==============================================================================

-- Find all events for a specific entity
SELECT
    timestamp,
    event_type,
    user_id,
    data
FROM fraiseql_events
WHERE entity_type = 'Order'
    AND entity_id = '550e8400-e29b-41d4-a716-446655440000'
ORDER BY timestamp ASC;

-- Find recent changes by user
SELECT
    timestamp,
    event_type,
    entity_type,
    entity_id,
    data
FROM fraiseql_events
WHERE user_id = 'user-123'
    AND timestamp >= now() - INTERVAL 24 HOUR
ORDER BY timestamp DESC
LIMIT 100;

-- Find events matching patterns in JSON data
SELECT
    timestamp,
    event_type,
    entity_type,
    entity_id,
    data
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 7 DAY
    AND JSONHas(data, 'error')
ORDER BY timestamp DESC;

-- ==============================================================================
-- PART 8: Maintenance and Monitoring
-- ==============================================================================

-- Check table sizes and parts
SELECT
    name,
    rows,
    bytes_on_disk,
    bytes_compressed
FROM system.parts
WHERE table = 'fraiseql_events'
ORDER BY modification_time DESC;

-- Check TTL status
SELECT
    table,
    rows,
    bytes_on_disk,
    modification_time
FROM system.parts
WHERE table = 'fraiseql_events'
ORDER BY modification_time DESC;

-- Monitor mutation operations
SELECT
    database,
    table,
    mutation_id,
    command,
    create_time,
    latest_failed_part,
    latest_fail_time
FROM system.mutations
WHERE database = currentDatabase()
ORDER BY create_time DESC;
