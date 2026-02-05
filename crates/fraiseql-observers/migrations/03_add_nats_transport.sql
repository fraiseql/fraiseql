-- FraiseQL Observer System - NATS Transport Schema
-- This migration adds support for NATS JetStream transport:
-- - Transport checkpoint table for cursor persistence
-- - NATS publication tracking columns on entity change log
-- - NOTIFY trigger for wake-up signals

-- ============================================================================
-- Transport Checkpoint Table
-- ============================================================================
-- Stores cursor position for event transports, enabling crash recovery.
-- Each transport (e.g., "pg_to_nats") maintains its own checkpoint.

CREATE TABLE IF NOT EXISTS core.tb_transport_checkpoint (
    -- Transport identifier (e.g., "pg_to_nats", "pg_to_kafka")
    transport_name TEXT PRIMARY KEY,

    -- Last processed primary key from source table
    last_pk BIGINT NOT NULL,

    -- When the checkpoint was last updated
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for monitoring/debugging queries (find stale checkpoints)
CREATE INDEX IF NOT EXISTS idx_transport_checkpoint_updated_at
ON core.tb_transport_checkpoint(updated_at DESC);

-- ============================================================================
-- Entity Change Log: NATS Publication Tracking
-- ============================================================================
-- Adds columns to track NATS publication status for each change log entry.
-- These columns enable:
-- - Idempotent publishing (skip already-published events)
-- - Race-safe conditional updates
-- - Debugging/monitoring of publication status

-- Add nats_published_at column (NULL = not yet published)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'core'
        AND table_name = 'tb_entity_change_log'
        AND column_name = 'nats_published_at'
    ) THEN
        ALTER TABLE core.tb_entity_change_log
            ADD COLUMN nats_published_at TIMESTAMP WITH TIME ZONE;
    END IF;
END $$;

-- Add nats_event_id column (UUID used for NATS deduplication)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'core'
        AND table_name = 'tb_entity_change_log'
        AND column_name = 'nats_event_id'
    ) THEN
        ALTER TABLE core.tb_entity_change_log
            ADD COLUMN nats_event_id UUID;
    END IF;
END $$;

-- Index for finding unpublished entries efficiently
-- Used by the bridge to batch fetch unpublished events
CREATE INDEX IF NOT EXISTS idx_entity_change_log_nats_unpublished
ON core.tb_entity_change_log(pk_entity_change_log)
WHERE nats_published_at IS NULL;

-- Index for monitoring published events by time
CREATE INDEX IF NOT EXISTS idx_entity_change_log_nats_published_at
ON core.tb_entity_change_log(nats_published_at)
WHERE nats_published_at IS NOT NULL;

-- ============================================================================
-- NOTIFY Trigger for Wake-Up Signals
-- ============================================================================
-- Sends PostgreSQL NOTIFY when new entity changes are inserted.
-- This is a performance optimization - the bridge also polls periodically
-- to ensure no events are missed even if NOTIFY is lost.

CREATE OR REPLACE FUNCTION core.notify_entity_change()
RETURNS TRIGGER AS $$
BEGIN
    -- Send NOTIFY with minimal payload (just a wake-up signal)
    -- The bridge fetches actual data via cursor query, not from NOTIFY payload
    PERFORM pg_notify('fraiseql_events', NEW.pk_entity_change_log::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger only if it doesn't exist
DROP TRIGGER IF EXISTS trigger_entity_change_notify ON core.tb_entity_change_log;
CREATE TRIGGER trigger_entity_change_notify
AFTER INSERT ON core.tb_entity_change_log
FOR EACH ROW
EXECUTE FUNCTION core.notify_entity_change();

-- ============================================================================
-- Monitoring View
-- ============================================================================
-- View for monitoring NATS publication status and lag

CREATE OR REPLACE VIEW core.vw_nats_publication_status AS
SELECT
    checkpoint.transport_name,
    checkpoint.last_pk AS checkpoint_cursor,
    checkpoint.updated_at AS checkpoint_updated_at,
    (SELECT MAX(pk_entity_change_log) FROM core.tb_entity_change_log) AS max_pk,
    (SELECT MAX(pk_entity_change_log) FROM core.tb_entity_change_log) - checkpoint.last_pk AS lag_count,
    (SELECT COUNT(*) FROM core.tb_entity_change_log WHERE nats_published_at IS NULL) AS unpublished_count
FROM core.tb_transport_checkpoint checkpoint
WHERE checkpoint.transport_name LIKE 'pg_to_nats%';

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE core.tb_transport_checkpoint IS
    'Stores cursor position for event transports (NATS bridge, etc.) for crash recovery';

COMMENT ON COLUMN core.tb_transport_checkpoint.transport_name IS
    'Unique identifier for the transport (e.g., "pg_to_nats")';

COMMENT ON COLUMN core.tb_transport_checkpoint.last_pk IS
    'Last processed pk_entity_change_log value';
