-- Phase 8.1: Persistent Checkpoints Table
-- Stores listener checkpoint state for recovery on restart

CREATE TABLE IF NOT EXISTS observer_checkpoints (
    listener_id VARCHAR(255) PRIMARY KEY,
    last_processed_id BIGINT NOT NULL DEFAULT 0,
    last_processed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    batch_size INT NOT NULL DEFAULT 100,
    event_count INT NOT NULL DEFAULT 0,
    consecutive_errors INT NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_by VARCHAR(255),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_ids CHECK (last_processed_id >= 0),
    CONSTRAINT valid_batch CHECK (batch_size > 0 AND batch_size <= 10000),
    CONSTRAINT valid_errors CHECK (consecutive_errors >= 0)
);

-- Index for recovery queries
CREATE INDEX IF NOT EXISTS idx_observer_checkpoints_updated_at
    ON observer_checkpoints(updated_at DESC);

-- Audit trail: Track all checkpoint updates
CREATE TABLE IF NOT EXISTS observer_checkpoints_history (
    id BIGSERIAL PRIMARY KEY,
    listener_id VARCHAR(255) NOT NULL,
    last_processed_id BIGINT NOT NULL,
    batch_size INT NOT NULL,
    event_count INT NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    reason VARCHAR(255),

    CONSTRAINT valid_history_ids CHECK (last_processed_id >= 0)
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_history_listener_id
    ON observer_checkpoints_history(listener_id);

CREATE INDEX IF NOT EXISTS idx_checkpoints_history_updated_at
    ON observer_checkpoints_history(updated_at DESC);

-- Trigger to auto-populate history (optional, but useful for debugging)
-- This creates an audit trail of all checkpoint changes
CREATE OR REPLACE FUNCTION observer_checkpoints_audit()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO observer_checkpoints_history (listener_id, last_processed_id, batch_size, event_count, reason)
    VALUES (NEW.listener_id, NEW.last_processed_id, NEW.batch_size, NEW.event_count, 'auto_update');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger only if it doesn't exist
DROP TRIGGER IF EXISTS observer_checkpoints_audit_trigger ON observer_checkpoints;
CREATE TRIGGER observer_checkpoints_audit_trigger
AFTER INSERT OR UPDATE ON observer_checkpoints
FOR EACH ROW
EXECUTE FUNCTION observer_checkpoints_audit();
