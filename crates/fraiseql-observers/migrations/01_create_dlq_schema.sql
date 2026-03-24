-- FraiseQL Observer System - Dead Letter Queue Schema
-- This migration creates the database schema for the observer system including:
-- - Event logging for debugging and audit trails
-- - Dead Letter Queue for failed actions
-- - DLQ history for retry tracking
--
-- All tables follow the Trinity Pattern:
--   pk_{entity} BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY
--   id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE
--   fk_{ref}    BIGINT REFERENCES tb_{ref}(pk_{ref})

-- ============================================================================
-- Observer Events Table
-- ============================================================================
-- Stores all events processed by the observer system for debugging and audit.

CREATE TABLE IF NOT EXISTS tb_observer_event (
    -- Trinity: internal PK
    pk_observer_event BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Trinity: external UUID
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Event type (INSERT, UPDATE, DELETE, CUSTOM)
    event_type VARCHAR(50) NOT NULL,

    -- Entity type name (e.g., "Order", "User", "Product")
    entity_type VARCHAR(100) NOT NULL,

    -- Entity instance ID
    entity_id UUID NOT NULL,

    -- Full event data as JSON
    data JSONB NOT NULL,

    -- When the event was recorded
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    -- When the event was processed (if completed)
    processed_at TIMESTAMP WITH TIME ZONE,

    -- Event status: pending, processing, completed, failed
    status VARCHAR(50) DEFAULT 'pending'
);

-- Index for efficient event lookup by entity type and event type
CREATE INDEX IF NOT EXISTS idx_tb_observer_event_entity
ON tb_observer_event(entity_type, event_type);

-- Index for status filtering
CREATE INDEX IF NOT EXISTS idx_tb_observer_event_status
ON tb_observer_event(status);

-- Index for time-range queries
CREATE INDEX IF NOT EXISTS idx_tb_observer_event_created
ON tb_observer_event(created_at);

-- ============================================================================
-- Dead Letter Queue Items Table
-- ============================================================================
-- Stores failed action executions for manual retry and debugging.

CREATE TABLE IF NOT EXISTS tb_observer_dlq_item (
    -- Trinity: internal PK
    pk_observer_dlq_item BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Trinity: external UUID
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Reference to the original event that failed (Trinity FK)
    fk_observer_event BIGINT NOT NULL REFERENCES tb_observer_event(pk_observer_event) ON DELETE CASCADE,

    -- Action type that failed (webhook, slack, email, sms, push, search, cache)
    action_type VARCHAR(50) NOT NULL,

    -- Action configuration as JSON
    action_config JSONB NOT NULL,

    -- Error message from the failure
    error_message TEXT NOT NULL,

    -- Current attempt count
    attempt_count INT DEFAULT 1,

    -- Maximum retry attempts
    max_attempts INT DEFAULT 3,

    -- When the item was added to DLQ
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    -- When the last retry attempt was made
    last_retry_at TIMESTAMP WITH TIME ZONE,

    -- Item status: pending, processing, success, retry_failed, manually_resolved
    status VARCHAR(50) DEFAULT 'pending'
);

-- Index for status filtering (find pending items)
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_item_status
ON tb_observer_dlq_item(status);

-- Index for time-based queries (find old items)
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_item_created
ON tb_observer_dlq_item(created_at);

-- Index for finding items by action type
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_item_action
ON tb_observer_dlq_item(action_type);

-- Index for finding items by event
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_item_event
ON tb_observer_dlq_item(fk_observer_event);

-- ============================================================================
-- Dead Letter Queue History Table
-- ============================================================================
-- Tracks all retry attempts and their results for audit and debugging.

CREATE TABLE IF NOT EXISTS tb_observer_dlq_history (
    -- Trinity: internal PK
    pk_observer_dlq_history BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Trinity: external UUID
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Reference to the DLQ item being retried (Trinity FK)
    fk_observer_dlq_item BIGINT NOT NULL REFERENCES tb_observer_dlq_item(pk_observer_dlq_item) ON DELETE CASCADE,

    -- Which retry attempt this was (1 = first attempt, 2 = first retry, etc.)
    attempt_number INT NOT NULL,

    -- Error message from this attempt (if failed)
    error_message TEXT NOT NULL,

    -- When this attempt was executed
    executed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    -- Result of this attempt: success, transient_error, permanent_error, timeout
    result VARCHAR(50) NOT NULL
);

-- Index for finding history by DLQ item
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_history_item
ON tb_observer_dlq_history(fk_observer_dlq_item);

-- Index for finding history by result (find failures)
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_history_result
ON tb_observer_dlq_history(result);

-- Index for time-based queries
CREATE INDEX IF NOT EXISTS idx_tb_observer_dlq_history_executed
ON tb_observer_dlq_history(executed_at);

-- ============================================================================
-- Views for Common Queries
-- ============================================================================

-- View: All pending retries (useful for monitoring dashboards)
CREATE OR REPLACE VIEW observer_pending_retries AS
SELECT
    dlq.pk_observer_dlq_item,
    dlq.id,
    dlq.fk_observer_event,
    dlq.action_type,
    dlq.error_message,
    dlq.attempt_count,
    dlq.max_attempts,
    dlq.created_at,
    dlq.last_retry_at,
    ev.entity_type,
    ev.event_type,
    ev.entity_id
FROM tb_observer_dlq_item dlq
JOIN tb_observer_event ev ON dlq.fk_observer_event = ev.pk_observer_event
WHERE dlq.status = 'pending'
ORDER BY dlq.created_at ASC;

-- View: Retry exhausted items (actions that failed all attempts)
CREATE OR REPLACE VIEW observer_retry_exhausted AS
SELECT
    dlq.pk_observer_dlq_item,
    dlq.id,
    dlq.fk_observer_event,
    dlq.action_type,
    dlq.error_message,
    dlq.attempt_count,
    dlq.max_attempts,
    dlq.created_at,
    ev.entity_type,
    ev.event_type,
    ev.entity_id
FROM tb_observer_dlq_item dlq
JOIN tb_observer_event ev ON dlq.fk_observer_event = ev.pk_observer_event
WHERE dlq.status = 'retry_failed'
AND dlq.attempt_count >= dlq.max_attempts
ORDER BY dlq.created_at DESC;

-- View: Recent failures (last 24 hours)
CREATE OR REPLACE VIEW observer_recent_failures AS
SELECT
    dlq.pk_observer_dlq_item,
    dlq.id,
    dlq.fk_observer_event,
    dlq.action_type,
    dlq.error_message,
    dlq.attempt_count,
    dlq.max_attempts,
    dlq.created_at,
    COUNT(hist.pk_observer_dlq_history) as retry_attempts,
    ev.entity_type,
    ev.event_type,
    ev.entity_id
FROM tb_observer_dlq_item dlq
LEFT JOIN tb_observer_dlq_history hist ON dlq.pk_observer_dlq_item = hist.fk_observer_dlq_item
JOIN tb_observer_event ev ON dlq.fk_observer_event = ev.pk_observer_event
WHERE dlq.created_at > NOW() - INTERVAL '24 hours'
GROUP BY dlq.pk_observer_dlq_item, ev.pk_observer_event
ORDER BY dlq.created_at DESC;

-- ============================================================================
-- Database Grants (if using separate application user)
-- ============================================================================
-- Uncomment these if your application uses a separate database user

-- GRANT SELECT, INSERT, UPDATE ON tb_observer_event TO app_user;
-- GRANT SELECT, INSERT, UPDATE ON tb_observer_dlq_item TO app_user;
-- GRANT SELECT, INSERT ON tb_observer_dlq_history TO app_user;
-- GRANT SELECT ON observer_pending_retries TO app_user;
-- GRANT SELECT ON observer_retry_exhausted TO app_user;
-- GRANT SELECT ON observer_recent_failures TO app_user;
