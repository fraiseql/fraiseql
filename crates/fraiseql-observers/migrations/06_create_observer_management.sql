-- FraiseQL Observer System - Observer Management Schema
-- This migration creates the database schema for observer management:
-- - tb_observer: Observer definitions (entity type, event type, actions)
-- - tb_observer_log: Execution logs for tracking and debugging
--
-- Follows the Trinity pattern:
-- - Table names: tb_<name> (singular)
-- - Primary keys: pk_<table_name>
-- - Foreign keys: fk_<referenced_table>

-- ============================================================================
-- Observer Definitions Table
-- ============================================================================
-- Stores observer configurations that define which events trigger which actions.

CREATE TABLE IF NOT EXISTS tb_observer (
    -- Primary key (Trinity pattern)
    pk_observer BIGSERIAL PRIMARY KEY,

    -- UUID for external reference (API responses, etc.)
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Observer name (human-readable identifier)
    name VARCHAR(255) NOT NULL,

    -- Description of what this observer does
    description TEXT,

    -- Entity type to observe (e.g., "Order", "User", "Product")
    -- NULL means observe all entity types
    entity_type VARCHAR(255),

    -- Event type to observe: INSERT, UPDATE, DELETE, or NULL for all
    event_type VARCHAR(50),

    -- Condition expression (optional filter, e.g., "status = 'completed'")
    -- Uses the condition DSL from fraiseql-observers
    condition_expression TEXT,

    -- Actions to execute as JSON array
    -- Example: [{"type": "webhook", "url": "...", "method": "POST"}, {"type": "email", ...}]
    actions JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- Whether this observer is currently enabled
    enabled BOOLEAN NOT NULL DEFAULT true,

    -- Priority for ordering (lower = higher priority)
    priority INTEGER NOT NULL DEFAULT 100,

    -- Retry configuration as JSON
    -- Example: {"max_attempts": 3, "backoff": "exponential", "initial_delay_ms": 1000}
    retry_config JSONB NOT NULL DEFAULT '{"max_attempts": 3, "backoff": "exponential", "initial_delay_ms": 1000}'::jsonb,

    -- Timeout for action execution (milliseconds)
    timeout_ms INTEGER NOT NULL DEFAULT 30000,

    -- Customer organization ID (for multi-tenancy, NULL = global)
    fk_customer_org BIGINT,

    -- Audit timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255),
    updated_by VARCHAR(255),

    -- Soft delete support
    deleted_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT ck_observer_event_type CHECK (
        event_type IS NULL OR event_type IN ('INSERT', 'UPDATE', 'DELETE', 'CUSTOM')
    ),
    CONSTRAINT ck_observer_actions_array CHECK (
        jsonb_typeof(actions) = 'array'
    ),
    CONSTRAINT ck_observer_timeout_positive CHECK (timeout_ms > 0),
    CONSTRAINT ck_observer_priority_range CHECK (priority >= 0 AND priority <= 1000)
);

-- Comments for documentation
COMMENT ON TABLE tb_observer IS 'Observer definitions that map entity events to actions';
COMMENT ON COLUMN tb_observer.pk_observer IS 'Internal primary key (Trinity pattern)';
COMMENT ON COLUMN tb_observer.id IS 'External UUID for API references';
COMMENT ON COLUMN tb_observer.entity_type IS 'Entity type to observe (NULL = all types)';
COMMENT ON COLUMN tb_observer.event_type IS 'Event type to observe (NULL = all events)';
COMMENT ON COLUMN tb_observer.condition_expression IS 'Optional filter condition in DSL format';
COMMENT ON COLUMN tb_observer.actions IS 'JSON array of action configurations';
COMMENT ON COLUMN tb_observer.retry_config IS 'Retry policy configuration';

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_observer_entity_type ON tb_observer(entity_type) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_observer_event_type ON tb_observer(event_type) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_observer_enabled ON tb_observer(enabled) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_observer_customer_org ON tb_observer(fk_customer_org) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_observer_priority ON tb_observer(priority) WHERE deleted_at IS NULL AND enabled = true;

-- Unique constraint on name per tenant (or global if no tenant)
CREATE UNIQUE INDEX IF NOT EXISTS idx_observer_name_unique
ON tb_observer(name, COALESCE(fk_customer_org, -1))
WHERE deleted_at IS NULL;

-- ============================================================================
-- Observer Execution Log Table
-- ============================================================================
-- Stores execution logs for each observer invocation for tracking and debugging.

CREATE TABLE IF NOT EXISTS tb_observer_log (
    -- Primary key (Trinity pattern)
    pk_observer_log BIGSERIAL PRIMARY KEY,

    -- UUID for external reference
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Reference to the observer that was executed
    fk_observer BIGINT NOT NULL REFERENCES tb_observer(pk_observer) ON DELETE CASCADE,

    -- Reference to the event that triggered this execution (from tb_entity_change_log)
    fk_entity_change_log BIGINT,

    -- Event ID (UUID) for correlation
    event_id UUID NOT NULL,

    -- Entity information
    entity_type VARCHAR(255) NOT NULL,
    entity_id UUID NOT NULL,
    event_type VARCHAR(50) NOT NULL,

    -- Execution status: pending, running, success, failed, skipped, timeout
    status VARCHAR(50) NOT NULL DEFAULT 'pending',

    -- Action that was executed (index in the actions array)
    action_index INTEGER,

    -- Action type (webhook, email, slack, etc.)
    action_type VARCHAR(50),

    -- Start and end timestamps for duration calculation
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Duration in milliseconds
    duration_ms INTEGER,

    -- Error information (if failed)
    error_code VARCHAR(100),
    error_message TEXT,
    error_details JSONB,

    -- Retry information
    attempt_number INTEGER NOT NULL DEFAULT 1,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMPTZ,

    -- Request/response for debugging (optional, can be disabled for privacy)
    request_payload JSONB,
    response_payload JSONB,
    response_status_code INTEGER,

    -- Trace context for distributed tracing
    trace_id VARCHAR(64),
    span_id VARCHAR(32),
    parent_span_id VARCHAR(32),

    -- Customer organization ID (denormalized for efficient querying)
    fk_customer_org BIGINT,

    -- When the log entry was created
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT ck_observer_log_status CHECK (
        status IN ('pending', 'running', 'success', 'failed', 'skipped', 'timeout', 'cancelled')
    ),
    CONSTRAINT ck_observer_log_attempt_positive CHECK (attempt_number > 0),
    CONSTRAINT ck_observer_log_duration_positive CHECK (duration_ms IS NULL OR duration_ms >= 0)
);

-- Comments for documentation
COMMENT ON TABLE tb_observer_log IS 'Execution logs for observer invocations';
COMMENT ON COLUMN tb_observer_log.pk_observer_log IS 'Internal primary key (Trinity pattern)';
COMMENT ON COLUMN tb_observer_log.fk_observer IS 'Reference to the observer definition';
COMMENT ON COLUMN tb_observer_log.fk_entity_change_log IS 'Reference to the triggering event';
COMMENT ON COLUMN tb_observer_log.status IS 'Execution status: pending, running, success, failed, skipped, timeout, cancelled';
COMMENT ON COLUMN tb_observer_log.attempt_number IS 'Current retry attempt (1 = first attempt)';

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_observer_log_observer ON tb_observer_log(fk_observer);
CREATE INDEX IF NOT EXISTS idx_observer_log_event_id ON tb_observer_log(event_id);
CREATE INDEX IF NOT EXISTS idx_observer_log_status ON tb_observer_log(status);
CREATE INDEX IF NOT EXISTS idx_observer_log_created_at ON tb_observer_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_observer_log_entity ON tb_observer_log(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_observer_log_customer_org ON tb_observer_log(fk_customer_org);
CREATE INDEX IF NOT EXISTS idx_observer_log_trace_id ON tb_observer_log(trace_id) WHERE trace_id IS NOT NULL;

-- Composite index for retry queries (find failed items that need retry)
CREATE INDEX IF NOT EXISTS idx_observer_log_retry
ON tb_observer_log(status, next_retry_at)
WHERE status = 'failed' AND next_retry_at IS NOT NULL;

-- ============================================================================
-- Observer Statistics View
-- ============================================================================
-- Aggregated statistics for observer execution monitoring.

CREATE OR REPLACE VIEW vw_observer_stats AS
SELECT
    o.pk_observer,
    o.id AS observer_id,
    o.name AS observer_name,
    o.entity_type,
    o.event_type,
    o.enabled,
    COUNT(l.pk_observer_log) AS total_executions,
    COUNT(CASE WHEN l.status = 'success' THEN 1 END) AS successful_executions,
    COUNT(CASE WHEN l.status = 'failed' THEN 1 END) AS failed_executions,
    COUNT(CASE WHEN l.status = 'timeout' THEN 1 END) AS timeout_executions,
    COUNT(CASE WHEN l.status = 'skipped' THEN 1 END) AS skipped_executions,
    ROUND(
        100.0 * COUNT(CASE WHEN l.status = 'success' THEN 1 END) /
        NULLIF(COUNT(l.pk_observer_log), 0),
        2
    ) AS success_rate_pct,
    AVG(l.duration_ms) AS avg_duration_ms,
    MAX(l.duration_ms) AS max_duration_ms,
    MIN(l.duration_ms) AS min_duration_ms,
    MAX(l.created_at) AS last_execution_at
FROM tb_observer o
LEFT JOIN tb_observer_log l ON o.pk_observer = l.fk_observer
WHERE o.deleted_at IS NULL
GROUP BY o.pk_observer, o.id, o.name, o.entity_type, o.event_type, o.enabled;

COMMENT ON VIEW vw_observer_stats IS 'Aggregated statistics for observer execution monitoring';

-- ============================================================================
-- Helper Functions
-- ============================================================================

-- Function to get active observers for an entity/event type combination
CREATE OR REPLACE FUNCTION fn_get_active_observers(
    p_entity_type VARCHAR(255),
    p_event_type VARCHAR(50),
    p_customer_org BIGINT DEFAULT NULL
)
RETURNS TABLE (
    pk_observer BIGINT,
    id UUID,
    name VARCHAR(255),
    condition_expression TEXT,
    actions JSONB,
    retry_config JSONB,
    timeout_ms INTEGER,
    priority INTEGER
)
LANGUAGE SQL
STABLE
AS $$
    SELECT
        o.pk_observer,
        o.id,
        o.name,
        o.condition_expression,
        o.actions,
        o.retry_config,
        o.timeout_ms,
        o.priority
    FROM tb_observer o
    WHERE o.deleted_at IS NULL
      AND o.enabled = true
      AND (o.entity_type IS NULL OR o.entity_type = p_entity_type)
      AND (o.event_type IS NULL OR o.event_type = p_event_type)
      AND (o.fk_customer_org IS NULL OR o.fk_customer_org = p_customer_org)
    ORDER BY o.priority ASC, o.pk_observer ASC;
$$;

COMMENT ON FUNCTION fn_get_active_observers IS 'Returns active observers matching the given entity/event type';

-- Function to log observer execution start
CREATE OR REPLACE FUNCTION fn_log_observer_start(
    p_fk_observer BIGINT,
    p_event_id UUID,
    p_entity_type VARCHAR(255),
    p_entity_id UUID,
    p_event_type VARCHAR(50),
    p_action_index INTEGER DEFAULT NULL,
    p_action_type VARCHAR(50) DEFAULT NULL,
    p_trace_id VARCHAR(64) DEFAULT NULL,
    p_span_id VARCHAR(32) DEFAULT NULL,
    p_fk_customer_org BIGINT DEFAULT NULL
)
RETURNS UUID
LANGUAGE SQL
AS $$
    INSERT INTO tb_observer_log (
        fk_observer,
        event_id,
        entity_type,
        entity_id,
        event_type,
        action_index,
        action_type,
        status,
        started_at,
        trace_id,
        span_id,
        fk_customer_org
    ) VALUES (
        p_fk_observer,
        p_event_id,
        p_entity_type,
        p_entity_id,
        p_event_type,
        p_action_index,
        p_action_type,
        'running',
        NOW(),
        p_trace_id,
        p_span_id,
        p_fk_customer_org
    )
    RETURNING id;
$$;

COMMENT ON FUNCTION fn_log_observer_start IS 'Creates a log entry when an observer starts execution';

-- Function to log observer execution completion
CREATE OR REPLACE FUNCTION fn_log_observer_complete(
    p_log_id UUID,
    p_status VARCHAR(50),
    p_error_code VARCHAR(100) DEFAULT NULL,
    p_error_message TEXT DEFAULT NULL,
    p_error_details JSONB DEFAULT NULL,
    p_response_payload JSONB DEFAULT NULL,
    p_response_status_code INTEGER DEFAULT NULL
)
RETURNS VOID
LANGUAGE SQL
AS $$
    UPDATE tb_observer_log
    SET
        status = p_status,
        completed_at = NOW(),
        duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
        error_code = p_error_code,
        error_message = p_error_message,
        error_details = p_error_details,
        response_payload = p_response_payload,
        response_status_code = p_response_status_code
    WHERE id = p_log_id;
$$;

COMMENT ON FUNCTION fn_log_observer_complete IS 'Updates a log entry when an observer completes execution';

-- ============================================================================
-- Trigger for updated_at timestamp
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_update_timestamp()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS tr_observer_updated_at ON tb_observer;
CREATE TRIGGER tr_observer_updated_at
    BEFORE UPDATE ON tb_observer
    FOR EACH ROW
    EXECUTE FUNCTION fn_update_timestamp();
