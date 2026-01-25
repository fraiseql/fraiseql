-- ClickHouse Events Table and Materialized Views
-- This migration creates the main fraiseql_events table and three materialized views
-- for analytics and observability.

-- ============================================================================
-- Main Events Table
-- ============================================================================

-- Create the main events table with MergeTree engine
-- Features:
--   - Partitioned by month for efficient retention and deletion
--   - Ordered by entity_type and timestamp for query optimization
--   - 90-day TTL for automatic data retention
--   - Bloom filter indexes on frequently-filtered columns
CREATE TABLE IF NOT EXISTS fraiseql_events
(
    `event_id` String COMMENT 'Unique event identifier',
    `event_type` String COMMENT 'Type of event (created, updated, deleted, custom)',
    `entity_type` String COMMENT 'Type of entity affected (User, Product, Order, etc.)',
    `entity_id` String COMMENT 'ID of the entity affected',
    `timestamp` Int64 COMMENT 'Unix timestamp in microseconds UTC',
    `data` String COMMENT 'Event data as JSON string',
    `user_id` Nullable(String) COMMENT 'User ID who triggered the event (nullable)',
    `org_id` Nullable(String) COMMENT 'Organization ID for multi-tenancy (nullable)',

    -- Index for user-based queries
    INDEX idx_user_id user_id TYPE bloom_filter GRANULARITY 1,
    -- Index for org-based queries
    INDEX idx_org_id org_id TYPE bloom_filter GRANULARITY 1,
    -- Index for event type queries
    INDEX idx_event_type event_type TYPE bloom_filter GRANULARITY 1
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(fromUnixTimestamp64Micro(timestamp))
ORDER BY (entity_type, timestamp)
SETTINGS index_granularity = 8192
TTL timestamp + INTERVAL 90 DAY
COMMENT 'Raw event log for FraiseQL analytics and observability';

-- ============================================================================
-- Materialized View 1: Hourly Event Counts by Type
-- ============================================================================

-- Creates an aggregated view of events rolled up hourly by event and entity type
CREATE TABLE IF NOT EXISTS fraiseql_events_hourly
(
    `hour` DateTime COMMENT 'Aggregation hour (UTC)',
    `event_type` String COMMENT 'Type of event',
    `entity_type` String COMMENT 'Type of entity',
    `event_count` UInt64 COMMENT 'Count of events',
    `unique_entities` UInt64 COMMENT 'Count of unique entity IDs'
)
ENGINE = SummingMergeTree()
ORDER BY (hour, event_type, entity_type)
COMMENT 'Hourly aggregation of event counts by type';

CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_events_hourly_mv
TO fraiseql_events_hourly
AS SELECT
    toStartOfHour(fromUnixTimestamp64Micro(timestamp)) AS hour,
    event_type,
    entity_type,
    CAST(count() AS UInt64) AS event_count,
    CAST(uniq(entity_id) AS UInt64) AS unique_entities
FROM fraiseql_events
GROUP BY hour, event_type, entity_type;

-- ============================================================================
-- Materialized View 2: Daily Organization Statistics
-- ============================================================================

-- Creates daily aggregate statistics per organization
CREATE TABLE IF NOT EXISTS fraiseql_org_daily
(
    `day` Date COMMENT 'Aggregation day (UTC)',
    `org_id` String COMMENT 'Organization ID',
    `event_count` UInt64 COMMENT 'Total events for org',
    `unique_users` UInt64 COMMENT 'Unique users who triggered events',
    `unique_entities` UInt64 COMMENT 'Unique entities affected',
    `created_count` UInt64 COMMENT 'CREATE events',
    `updated_count` UInt64 COMMENT 'UPDATE events',
    `deleted_count` UInt64 COMMENT 'DELETE events'
)
ENGINE = SummingMergeTree()
ORDER BY (day, org_id)
COMMENT 'Daily organization-level event statistics';

CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_org_daily_mv
TO fraiseql_org_daily
AS SELECT
    toDate(fromUnixTimestamp64Micro(timestamp)) AS day,
    COALESCE(org_id, 'unknown') AS org_id,
    CAST(count() AS UInt64) AS event_count,
    CAST(uniq(user_id) AS UInt64) AS unique_users,
    CAST(uniq(entity_id) AS UInt64) AS unique_entities,
    CAST(sumIf(1, event_type = 'created') AS UInt64) AS created_count,
    CAST(sumIf(1, event_type = 'updated') AS UInt64) AS updated_count,
    CAST(sumIf(1, event_type = 'deleted') AS UInt64) AS deleted_count
FROM fraiseql_events
GROUP BY day, org_id;

-- ============================================================================
-- Materialized View 3: Event Type Statistics with Hourly Rollup
-- ============================================================================

-- Creates a summary of event types with performance metrics
CREATE TABLE IF NOT EXISTS fraiseql_event_type_stats
(
    `hour` DateTime COMMENT 'Aggregation hour (UTC)',
    `event_type` String COMMENT 'Type of event',
    `event_count` UInt64 COMMENT 'Count of events',
    `avg_data_size_bytes` UInt64 COMMENT 'Average data JSON size',
    `max_data_size_bytes` UInt64 COMMENT 'Max data JSON size'
)
ENGINE = SummingMergeTree()
ORDER BY (hour, event_type)
COMMENT 'Hourly event type statistics with data size metrics';

CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_event_type_stats_mv
TO fraiseql_event_type_stats
AS SELECT
    toStartOfHour(fromUnixTimestamp64Micro(timestamp)) AS hour,
    event_type,
    CAST(count() AS UInt64) AS event_count,
    CAST(avg(length(data)) AS UInt64) AS avg_data_size_bytes,
    CAST(max(length(data)) AS UInt64) AS max_data_size_bytes
FROM fraiseql_events
GROUP BY hour, event_type;
