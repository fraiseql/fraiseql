-- FraiseQL PostgreSQL-Native Observability Schema
-- This schema extends tb_entity_change_log pattern to errors, traces, and metrics

-- ============================================================================
-- ERROR TRACKING (Sentry replacement)
-- ============================================================================

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

-- ============================================================================
-- ERROR OCCURRENCES (Individual error instances)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_error_occurrence (
    occurrence_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    error_id UUID NOT NULL REFERENCES tb_error_log(error_id) ON DELETE CASCADE,

    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Full context for this specific occurrence
    request_context JSONB,
    user_context JSONB,
    stack_trace TEXT,

    -- Breadcrumbs (user actions leading to error)
    breadcrumbs JSONB DEFAULT '[]'::jsonb,

    -- OpenTelemetry
    trace_id TEXT,
    span_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_occurrence_error ON tb_error_occurrence(error_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_occurrence_trace ON tb_error_occurrence(trace_id) WHERE trace_id IS NOT NULL;

-- ============================================================================
-- OPENTELEMETRY TRACES (in PostgreSQL)
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

    PRIMARY KEY (trace_id, span_id)
);

-- Indexes for trace queries
CREATE INDEX IF NOT EXISTS idx_otel_trace_time ON otel_traces(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_otel_trace_operation ON otel_traces(operation_name, start_time DESC);
CREATE INDEX IF NOT EXISTS idx_otel_trace_service ON otel_traces(service_name, start_time DESC);
CREATE INDEX IF NOT EXISTS idx_otel_trace_parent ON otel_traces(trace_id, parent_span_id);
CREATE INDEX IF NOT EXISTS idx_otel_trace_duration ON otel_traces(duration_ms DESC) WHERE duration_ms IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_otel_trace_errors ON otel_traces(status_code) WHERE status_code = 'error';

-- GIN index for attribute searching
CREATE INDEX IF NOT EXISTS idx_otel_attributes ON otel_traces USING gin(attributes);

-- ============================================================================
-- OPENTELEMETRY METRICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS otel_metrics (
    metric_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

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
    bucket_bounds JSONB, -- for histogram
    quantiles JSONB -- for summary
);

CREATE INDEX IF NOT EXISTS idx_otel_metrics_name_time ON otel_metrics(metric_name, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_otel_metrics_time ON otel_metrics(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_otel_metrics_labels ON otel_metrics USING gin(labels);

-- ============================================================================
-- ERROR NOTIFICATIONS (extensible notification system)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_error_notification_config (
    config_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- When to notify
    error_fingerprint TEXT, -- NULL = all errors
    error_type TEXT, -- NULL = all types
    severity TEXT[], -- array of severities to notify on
    environment TEXT[], -- array of environments
    min_occurrence_count INT DEFAULT 1,

    -- Notification settings
    enabled BOOLEAN DEFAULT true,
    channel_type TEXT NOT NULL, -- email, slack, webhook, sms
    channel_config JSONB NOT NULL, -- channel-specific configuration

    -- Rate limiting
    rate_limit_minutes INT DEFAULT 60, -- don't send more than once per hour for same error

    -- Template
    message_template TEXT,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by TEXT,
    last_triggered TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_notification_config_enabled ON tb_error_notification_config(enabled) WHERE enabled = true;

-- Table to track sent notifications
CREATE TABLE IF NOT EXISTS tb_error_notification_log (
    notification_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    config_id UUID REFERENCES tb_error_notification_config(config_id) ON DELETE CASCADE,
    error_id UUID REFERENCES tb_error_log(error_id) ON DELETE CASCADE,

    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    channel_type TEXT NOT NULL,
    recipient TEXT NOT NULL,

    -- Status
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed')),
    error_message TEXT,

    -- Rate limiting tracking
    CONSTRAINT unique_error_config_ratelimit UNIQUE (error_id, config_id, sent_at)
);

CREATE INDEX IF NOT EXISTS idx_notification_log_error ON tb_error_notification_log(error_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_notification_log_status ON tb_error_notification_log(status) WHERE status = 'failed';

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
    -- Recent occurrence count
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

-- Function to resolve an error
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

-- Function to get error statistics
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
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE tb_error_log IS 'PostgreSQL-native error tracking - Sentry replacement';
COMMENT ON TABLE tb_error_occurrence IS 'Individual error occurrences with full context';
COMMENT ON TABLE otel_traces IS 'OpenTelemetry distributed traces stored in PostgreSQL';
COMMENT ON TABLE otel_metrics IS 'OpenTelemetry metrics stored in PostgreSQL';
COMMENT ON TABLE tb_error_notification_config IS 'Configuration for error notifications (email, Slack, etc.)';
COMMENT ON TABLE tb_error_notification_log IS 'Log of sent error notifications';

COMMENT ON COLUMN tb_error_log.error_fingerprint IS 'Hash of error type + file + line for grouping';
COMMENT ON COLUMN tb_error_log.occurrence_count IS 'Total number of times this error has occurred';
COMMENT ON COLUMN tb_error_log.trace_id IS 'OpenTelemetry trace ID for correlation';
