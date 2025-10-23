-- FraiseQL PostgreSQL-Native Observability Schema (Partitioned Version)
-- This schema uses native PostgreSQL declarative partitioning for scalability
--
-- DESIGN DECISIONS:
-- - Monthly partitioning for tb_error_occurrence (high write volume)
-- - tb_error_log remains unpartitioned (low volume, needs unique constraints)
-- - Automatic partition creation via function + cron/pg_cron
-- - 6-month retention with automatic archival

-- ============================================================================
-- SCHEMA VERSION TRACKING
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraiseql_schema_version (
    module TEXT PRIMARY KEY,
    version INT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    description TEXT
);

INSERT INTO fraiseql_schema_version (module, version, description)
VALUES ('monitoring', 1, 'Initial partitioned schema')
ON CONFLICT (module) DO NOTHING;

-- ============================================================================
-- ERROR TRACKING - SUMMARY TABLE (Unpartitioned)
-- ============================================================================
-- This table stores error fingerprints and aggregated data.
-- It remains unpartitioned for fast lookups and unique constraints.

CREATE TABLE IF NOT EXISTS tb_error_log (
    error_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Error identification (for grouping similar errors)
    error_fingerprint TEXT NOT NULL,
    error_type TEXT NOT NULL,
    error_message TEXT NOT NULL,
    stack_trace TEXT,

    -- Context (request, user, app state)
    request_context JSONB DEFAULT '{}'::jsonb,
    application_context JSONB DEFAULT '{}'::jsonb,
    user_context JSONB DEFAULT '{}'::jsonb,

    -- Occurrence tracking
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    occurrence_count INT DEFAULT 1,

    -- Issue management
    status TEXT DEFAULT 'unresolved' CHECK (status IN ('unresolved', 'resolved', 'ignored', 'investigating')),
    assigned_to TEXT,
    resolved_at TIMESTAMPTZ,
    resolved_by TEXT,
    resolution_notes TEXT,

    -- OpenTelemetry correlation
    trace_id TEXT,
    span_id TEXT,

    -- Severity
    severity TEXT DEFAULT 'error' CHECK (severity IN ('debug', 'info', 'warning', 'error', 'critical')),

    -- Tags for categorization
    tags JSONB DEFAULT '[]'::jsonb,

    -- Environment
    environment TEXT DEFAULT 'production',
    release_version TEXT,

    CONSTRAINT unique_fingerprint UNIQUE (error_fingerprint)
);

-- Indexes for fast queries
CREATE INDEX IF NOT EXISTS idx_error_fingerprint ON tb_error_log(error_fingerprint);
CREATE INDEX IF NOT EXISTS idx_error_unresolved ON tb_error_log(status, last_seen) WHERE status = 'unresolved';
CREATE INDEX IF NOT EXISTS idx_error_trace ON tb_error_log(trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_error_severity ON tb_error_log(severity, last_seen);
CREATE INDEX IF NOT EXISTS idx_error_type ON tb_error_log(error_type, last_seen);
CREATE INDEX IF NOT EXISTS idx_error_environment ON tb_error_log(environment, status);
CREATE INDEX IF NOT EXISTS idx_error_user ON tb_error_log((user_context->>'user_id')) WHERE user_context->>'user_id' IS NOT NULL;

-- GIN index for JSONB searching
CREATE INDEX IF NOT EXISTS idx_error_tags ON tb_error_log USING gin(tags);
CREATE INDEX IF NOT EXISTS idx_error_request_context ON tb_error_log USING gin(request_context);

COMMENT ON TABLE tb_error_log IS 'PostgreSQL-native error tracking - Aggregated error summaries (unpartitioned)';
COMMENT ON COLUMN tb_error_log.error_fingerprint IS 'Hash of error type + file + line for grouping';
COMMENT ON COLUMN tb_error_log.occurrence_count IS 'Total number of times this error has occurred';

-- ============================================================================
-- ERROR OCCURRENCES - PARTITIONED TABLE
-- ============================================================================
-- Individual error instances partitioned by month for scalability.
-- High-volume writes benefit from partition pruning and parallel queries.

CREATE TABLE IF NOT EXISTS tb_error_occurrence (
    occurrence_id UUID NOT NULL DEFAULT gen_random_uuid(),
    error_id UUID NOT NULL,  -- No FK constraint across partitions

    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Full context for this specific occurrence
    request_context JSONB,
    user_context JSONB,
    stack_trace TEXT,

    -- Breadcrumbs (user actions leading to error)
    breadcrumbs JSONB DEFAULT '[]'::jsonb,

    -- OpenTelemetry
    trace_id TEXT,
    span_id TEXT,

    PRIMARY KEY (occurrence_id, occurred_at)  -- Must include partition key
) PARTITION BY RANGE (occurred_at);

-- Create indexes on parent table (inherited by all partitions)
CREATE INDEX IF NOT EXISTS idx_occurrence_error_time ON tb_error_occurrence(error_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_occurrence_trace ON tb_error_occurrence(trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_occurrence_time ON tb_error_occurrence(occurred_at DESC);

COMMENT ON TABLE tb_error_occurrence IS 'Individual error occurrences (partitioned by month)';

-- ============================================================================
-- PARTITION MANAGEMENT
-- ============================================================================

-- Function to create a partition for a specific month
CREATE OR REPLACE FUNCTION create_error_occurrence_partition(
    partition_date DATE
) RETURNS TEXT AS $$
DECLARE
    partition_name TEXT;
    start_date DATE;
    end_date DATE;
BEGIN
    -- Calculate partition bounds (first day of month to first day of next month)
    start_date := DATE_TRUNC('month', partition_date)::DATE;
    end_date := (DATE_TRUNC('month', partition_date) + INTERVAL '1 month')::DATE;

    -- Generate partition name: tb_error_occurrence_2024_01
    partition_name := 'tb_error_occurrence_' || TO_CHAR(partition_date, 'YYYY_MM');

    -- Create partition if it doesn't exist
    EXECUTE format(
        'CREATE TABLE IF NOT EXISTS %I PARTITION OF tb_error_occurrence
         FOR VALUES FROM (%L) TO (%L)',
        partition_name,
        start_date,
        end_date
    );

    RETURN partition_name;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION create_error_occurrence_partition IS 'Create monthly partition for error occurrences';

-- Function to automatically create partitions (call from cron or trigger)
CREATE OR REPLACE FUNCTION ensure_error_occurrence_partitions(
    months_ahead INT DEFAULT 2
) RETURNS TABLE (partition_name TEXT, created BOOLEAN) AS $$
DECLARE
    current_month DATE;
    target_month DATE;
    i INT;
    part_name TEXT;
    part_exists BOOLEAN;
BEGIN
    current_month := DATE_TRUNC('month', CURRENT_DATE)::DATE;

    -- Create partitions for current month + N months ahead
    FOR i IN 0..months_ahead LOOP
        target_month := current_month + (i || ' months')::INTERVAL;
        part_name := 'tb_error_occurrence_' || TO_CHAR(target_month, 'YYYY_MM');

        -- Check if partition exists
        SELECT EXISTS (
            SELECT 1 FROM pg_tables
            WHERE schemaname = 'public' AND tablename = part_name
        ) INTO part_exists;

        IF NOT part_exists THEN
            PERFORM create_error_occurrence_partition(target_month);
            partition_name := part_name;
            created := TRUE;
            RETURN NEXT;
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION ensure_error_occurrence_partitions IS 'Ensure partitions exist for current and future months';

-- Function to drop old partitions (data retention policy)
CREATE OR REPLACE FUNCTION drop_old_error_occurrence_partitions(
    retention_months INT DEFAULT 6
) RETURNS TABLE (partition_name TEXT, dropped BOOLEAN) AS $$
DECLARE
    cutoff_date DATE;
    part_record RECORD;
BEGIN
    cutoff_date := (DATE_TRUNC('month', CURRENT_DATE) - (retention_months || ' months')::INTERVAL)::DATE;

    -- Find and drop old partitions
    FOR part_record IN
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = 'public'
        AND tablename LIKE 'tb_error_occurrence_%'
        AND tablename ~ '^\w+_\d{4}_\d{2}$'  -- Match pattern: prefix_YYYY_MM
    LOOP
        -- Extract date from partition name
        DECLARE
            part_date DATE;
            year_month TEXT;
        BEGIN
            year_month := SUBSTRING(part_record.tablename FROM '\d{4}_\d{2}$');
            part_date := TO_DATE(year_month, 'YYYY_MM');

            IF part_date < cutoff_date THEN
                EXECUTE format('DROP TABLE IF EXISTS %I', part_record.tablename);
                partition_name := part_record.tablename;
                dropped := TRUE;
                RETURN NEXT;
            END IF;
        END;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION drop_old_error_occurrence_partitions IS 'Drop partitions older than retention period (default: 6 months)';

-- Create initial partitions (current month + 2 months ahead)
SELECT ensure_error_occurrence_partitions(2);

-- ============================================================================
-- OPENTELEMETRY TRACES (Partitioned by day for high-volume tracing)
-- ============================================================================

CREATE TABLE IF NOT EXISTS otel_traces (
    trace_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    parent_span_id TEXT,

    -- Span metadata
    operation_name TEXT NOT NULL,
    service_name TEXT NOT NULL,
    span_kind TEXT, -- server, client, producer, consumer, internal

    -- Timing
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    duration_ms INT,

    -- Status
    status_code TEXT, -- ok, error, unset
    status_message TEXT,

    -- Attributes
    attributes JSONB DEFAULT '{}'::jsonb,
    resource_attributes JSONB DEFAULT '{}'::jsonb,

    -- Events (logs within span)
    events JSONB DEFAULT '[]'::jsonb,

    -- Links to other spans
    links JSONB DEFAULT '[]'::jsonb,

    PRIMARY KEY (trace_id, span_id, start_time)
) PARTITION BY RANGE (start_time);

-- Indexes for trace queries
CREATE INDEX IF NOT EXISTS idx_otel_trace_time ON otel_traces(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_otel_trace_operation ON otel_traces(operation_name, start_time DESC);
CREATE INDEX IF NOT EXISTS idx_otel_trace_service ON otel_traces(service_name, start_time DESC);
CREATE INDEX IF NOT EXISTS idx_otel_trace_parent ON otel_traces(trace_id, parent_span_id);
CREATE INDEX IF NOT EXISTS idx_otel_trace_duration ON otel_traces(duration_ms DESC) WHERE duration_ms IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_otel_trace_errors ON otel_traces(status_code) WHERE status_code = 'error';
CREATE INDEX IF NOT EXISTS idx_otel_attributes ON otel_traces USING gin(attributes);

COMMENT ON TABLE otel_traces IS 'OpenTelemetry distributed traces (partitioned by day)';

-- ============================================================================
-- OPENTELEMETRY METRICS (Partitioned by day)
-- ============================================================================

CREATE TABLE IF NOT EXISTS otel_metrics (
    metric_id UUID NOT NULL DEFAULT gen_random_uuid(),

    -- Metric identification
    metric_name TEXT NOT NULL,
    metric_type TEXT NOT NULL, -- counter, gauge, histogram, summary

    -- Value
    value DOUBLE PRECISION NOT NULL,

    -- Timing
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Labels/Tags
    labels JSONB DEFAULT '{}'::jsonb,
    resource_attributes JSONB DEFAULT '{}'::jsonb,

    -- Histogram/Summary specific
    bucket_bounds JSONB,
    quantiles JSONB,

    PRIMARY KEY (metric_id, timestamp)
) PARTITION BY RANGE (timestamp);

CREATE INDEX IF NOT EXISTS idx_otel_metrics_name_time ON otel_metrics(metric_name, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_otel_metrics_time ON otel_metrics(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_otel_metrics_labels ON otel_metrics USING gin(labels);

COMMENT ON TABLE otel_metrics IS 'OpenTelemetry metrics (partitioned by day)';

-- ============================================================================
-- ERROR NOTIFICATIONS (Unpartitioned - low volume configuration data)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_error_notification_config (
    config_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- When to notify
    error_fingerprint TEXT,
    error_type TEXT,
    severity TEXT[],
    environment TEXT[],
    min_occurrence_count INT DEFAULT 1,

    -- Notification settings
    enabled BOOLEAN DEFAULT true,
    channel_type TEXT NOT NULL,
    channel_config JSONB NOT NULL,

    -- Rate limiting
    rate_limit_minutes INT DEFAULT 60,

    -- Template
    message_template TEXT,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by TEXT,
    last_triggered TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_notification_config_enabled ON tb_error_notification_config(enabled) WHERE enabled = true;

-- Notification delivery log (partitioned by month)
CREATE TABLE IF NOT EXISTS tb_error_notification_log (
    notification_id UUID NOT NULL DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL,
    error_id UUID NOT NULL,

    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    channel_type TEXT NOT NULL,
    recipient TEXT NOT NULL,

    -- Status
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed')),
    error_message TEXT,

    PRIMARY KEY (notification_id, sent_at)
) PARTITION BY RANGE (sent_at);

CREATE INDEX IF NOT EXISTS idx_notification_log_error_time ON tb_error_notification_log(error_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_notification_log_status ON tb_error_notification_log(status) WHERE status = 'failed';

COMMENT ON TABLE tb_error_notification_config IS 'Configuration for error notifications';
COMMENT ON TABLE tb_error_notification_log IS 'Notification delivery log (partitioned by month)';

-- ============================================================================
-- VIEWS FOR COMMON QUERIES
-- ============================================================================

-- Active errors (unresolved, seen in last 24 hours)
CREATE OR REPLACE VIEW v_active_errors AS
SELECT
    el.error_id,
    el.error_type,
    el.error_message,
    el.severity,
    el.occurrence_count,
    el.first_seen,
    el.last_seen,
    el.environment,
    el.trace_id,
    COUNT(eo.occurrence_id) FILTER (WHERE eo.occurred_at > NOW() - INTERVAL '24 hours') as recent_occurrences
FROM tb_error_log el
LEFT JOIN tb_error_occurrence eo ON el.error_id = eo.error_id
WHERE el.status = 'unresolved'
    AND el.last_seen > NOW() - INTERVAL '24 hours'
GROUP BY el.error_id
ORDER BY el.last_seen DESC;

-- Error trends (errors per hour for last 24 hours)
CREATE OR REPLACE VIEW v_error_trends AS
SELECT
    date_trunc('hour', eo.occurred_at) as hour,
    el.error_type,
    el.severity,
    COUNT(*) as error_count
FROM tb_error_occurrence eo
JOIN tb_error_log el ON eo.error_id = el.error_id
WHERE eo.occurred_at > NOW() - INTERVAL '24 hours'
GROUP BY date_trunc('hour', eo.occurred_at), el.error_type, el.severity
ORDER BY hour DESC, error_count DESC;

-- Top errors by occurrence
CREATE OR REPLACE VIEW v_top_errors AS
SELECT
    el.error_id,
    el.error_type,
    el.error_message,
    el.severity,
    el.occurrence_count,
    el.last_seen,
    el.status
FROM tb_error_log el
WHERE el.first_seen > NOW() - INTERVAL '7 days'
ORDER BY el.occurrence_count DESC
LIMIT 100;

-- Slow traces (p95 by operation)
CREATE OR REPLACE VIEW v_slow_traces AS
SELECT
    operation_name,
    service_name,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95_duration_ms,
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY duration_ms) as p50_duration_ms,
    COUNT(*) as trace_count,
    MAX(start_time) as last_seen
FROM otel_traces
WHERE start_time > NOW() - INTERVAL '1 hour'
    AND duration_ms IS NOT NULL
GROUP BY operation_name, service_name
HAVING COUNT(*) >= 10
ORDER BY p95_duration_ms DESC;

-- ============================================================================
-- FUNCTIONS FOR ERROR MANAGEMENT
-- ============================================================================

CREATE OR REPLACE FUNCTION resolve_error(
    p_error_id UUID,
    p_resolved_by TEXT,
    p_resolution_notes TEXT DEFAULT NULL
) RETURNS VOID AS $$
BEGIN
    UPDATE tb_error_log
    SET status = 'resolved',
        resolved_at = NOW(),
        resolved_by = p_resolved_by,
        resolution_notes = p_resolution_notes
    WHERE error_id = p_error_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION get_error_stats(
    p_hours INT DEFAULT 24
) RETURNS TABLE (
    total_errors BIGINT,
    unresolved_errors BIGINT,
    unique_error_types BIGINT,
    avg_resolution_time_hours NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        COUNT(*)::BIGINT as total_errors,
        COUNT(*) FILTER (WHERE status = 'unresolved')::BIGINT as unresolved_errors,
        COUNT(DISTINCT error_type)::BIGINT as unique_error_types,
        AVG(EXTRACT(EPOCH FROM (resolved_at - first_seen)) / 3600)::NUMERIC as avg_resolution_time_hours
    FROM tb_error_log
    WHERE first_seen > NOW() - (p_hours || ' hours')::INTERVAL;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- MAINTENANCE HELPER
-- ============================================================================

-- Function to get partition statistics
CREATE OR REPLACE FUNCTION get_partition_stats()
RETURNS TABLE (
    table_name TEXT,
    partition_name TEXT,
    row_count BIGINT,
    total_size TEXT,
    index_size TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        parent.relname::TEXT as table_name,
        child.relname::TEXT as partition_name,
        pg_stat_get_tuples_returned(child.oid)::BIGINT as row_count,
        pg_size_pretty(pg_total_relation_size(child.oid)) as total_size,
        pg_size_pretty(pg_indexes_size(child.oid)) as index_size
    FROM pg_inherits
    JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
    JOIN pg_class child ON pg_inherits.inhrelid = child.oid
    WHERE parent.relname IN ('tb_error_occurrence', 'otel_traces', 'otel_metrics', 'tb_error_notification_log')
    ORDER BY parent.relname, child.relname;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_partition_stats IS 'Get statistics for all partitioned tables';
