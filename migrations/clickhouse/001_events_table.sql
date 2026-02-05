-- ClickHouse schema for FraiseQL observer events
-- This migration creates the main events table and materialized views for analytics

-- Main events table: stores all observer events in columnar format
CREATE TABLE IF NOT EXISTS fraiseql_events (
    event_id String,
    event_type String,
    entity_type String,
    entity_id String,
    timestamp DateTime('UTC'),  -- Stored as DateTime for efficient aggregations
    data String,  -- Event data as JSON
    user_id Nullable(String),
    org_id Nullable(String)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (entity_type, timestamp)
TTL timestamp + INTERVAL 90 DAY  -- Auto-delete events older than 90 days
SETTINGS
    index_granularity = 8192,
    index_granularity_bytes = 10485760
;

-- Create skipping indexes for fast filtering
CREATE INDEX IF NOT EXISTS event_type_idx ON fraiseql_events (event_type) TYPE bloom_filter GRANULARITY 1;
CREATE INDEX IF NOT EXISTS entity_type_idx ON fraiseql_events (entity_type) TYPE bloom_filter GRANULARITY 1;
CREATE INDEX IF NOT EXISTS org_id_idx ON fraiseql_events (org_id) TYPE bloom_filter GRANULARITY 1;

-- Materialized view: hourly event counts by entity and event type
CREATE TABLE IF NOT EXISTS fraiseql_events_hourly (
    hour DateTime,
    entity_type String,
    event_type String,
    event_count UInt64,
    unique_entities UInt64
)
ENGINE = SummingMergeTree((event_count, unique_entities))
ORDER BY (hour, entity_type, event_type)
TTL hour + INTERVAL 120 DAY  -- Keep 120 days of hourly data
;

-- Populate hourly aggregations from events
CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_events_hourly_mv TO fraiseql_events_hourly AS
SELECT
    toStartOfHour(timestamp) as hour,
    entity_type,
    event_type,
    count() as event_count,
    uniq(entity_id) as unique_entities
FROM fraiseql_events
GROUP BY toStartOfHour(timestamp), entity_type, event_type
;

-- Materialized view: daily organization statistics
CREATE TABLE IF NOT EXISTS fraiseql_org_daily (
    day DateTime,
    org_id Nullable(String),
    event_count UInt64,
    unique_entities UInt64,
    unique_users UInt64,
    entity_types AggregateFunction(groupUniq(10), String)
)
ENGINE = SummingMergeTree((event_count, unique_entities, unique_users))
ORDER BY (day, org_id)
TTL day + INTERVAL 90 DAY
;

-- Populate daily organization stats from events
CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_org_daily_mv TO fraiseql_org_daily AS
SELECT
    toStartOfDay(timestamp) as day,
    org_id,
    count() as event_count,
    uniq(entity_id) as unique_entities,
    uniq(user_id) as unique_users,
    groupUniqState(10)(entity_type) as entity_types
FROM fraiseql_events
WHERE org_id IS NOT NULL
GROUP BY toStartOfDay(timestamp), org_id
;

-- Materialized view: event type distribution with hourly granularity
CREATE TABLE IF NOT EXISTS fraiseql_event_type_stats (
    hour DateTime,
    event_type String,
    count UInt64,
    rate Float64  -- Events per second
)
ENGINE = SummingMergeTree((count))
ORDER BY (hour, event_type)
TTL hour + INTERVAL 120 DAY
;

-- Populate event type stats from events
CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_event_type_stats_mv TO fraiseql_event_type_stats AS
SELECT
    toStartOfHour(timestamp) as hour,
    event_type,
    count() as count,
    count() / 3600.0 as rate  -- Divide by seconds in hour
FROM fraiseql_events
GROUP BY toStartOfHour(timestamp), event_type
;

-- Helper function for common queries: get event count by entity type in last N hours
CREATE FUNCTION IF NOT EXISTS count_events_by_entity_type(hours Int32 = 24)
RETURNS Table(entity_type String, event_count UInt64) AS
SELECT
    entity_type,
    sum(event_count) as event_count
FROM fraiseql_events_hourly
WHERE hour >= now() - INTERVAL hours HOUR
GROUP BY entity_type
ORDER BY event_count DESC
;

-- Helper function: get org activity summary
CREATE FUNCTION IF NOT EXISTS org_activity_summary(org_id_str String, days Int32 = 30)
RETURNS Table(day DateTime, event_count UInt64, unique_entities UInt64) AS
SELECT
    day,
    event_count,
    unique_entities
FROM fraiseql_org_daily
WHERE org_id = org_id_str AND day >= now() - INTERVAL days DAY
ORDER BY day DESC
;

-- Helper function: get recent events for an entity
CREATE FUNCTION IF NOT EXISTS get_entity_events(entity_id_str String, limit UInt32 = 100)
RETURNS Table(event_id String, event_type String, timestamp DateTime, data String) AS
SELECT
    event_id,
    event_type,
    timestamp,
    data
FROM fraiseql_events
WHERE entity_id = entity_id_str
ORDER BY timestamp DESC
LIMIT limit
;
