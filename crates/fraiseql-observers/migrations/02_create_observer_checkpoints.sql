-- Phase 8.1: Persistent Checkpoints Table
-- Stores listener checkpoint state for recovery on restart

CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.observer_checkpoints (
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
-- Each statement is guarded by the catalog: a boot over a current schema takes no
-- table lock, where `IF NOT EXISTS` alone still would.
DO $checkpoint_indexes$
BEGIN
    IF to_regclass('core.idx_observer_checkpoints_updated_at') IS NULL THEN
        CREATE INDEX idx_observer_checkpoints_updated_at ON core.observer_checkpoints (updated_at DESC);
    END IF;
END
$checkpoint_indexes$;

-- Audit trail: Track all checkpoint updates
CREATE TABLE IF NOT EXISTS core.observer_checkpoints_history (
    id BIGSERIAL PRIMARY KEY,
    listener_id VARCHAR(255) NOT NULL,
    last_processed_id BIGINT NOT NULL,
    batch_size INT NOT NULL,
    event_count INT NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    reason VARCHAR(255),

    CONSTRAINT valid_history_ids CHECK (last_processed_id >= 0)
);

-- Each statement is guarded by the catalog: a boot over a current schema takes no
-- table lock, where `IF NOT EXISTS` alone still would.
DO $checkpoint_history_indexes$
BEGIN
    IF to_regclass('core.idx_checkpoints_history_listener_id') IS NULL THEN
        CREATE INDEX idx_checkpoints_history_listener_id ON core.observer_checkpoints_history (listener_id);
    END IF;

    IF to_regclass('core.idx_checkpoints_history_updated_at') IS NULL THEN
        CREATE INDEX idx_checkpoints_history_updated_at ON core.observer_checkpoints_history (updated_at DESC);
    END IF;
END
$checkpoint_history_indexes$;

-- Trigger to auto-populate history (optional, but useful for debugging)
-- This creates an audit trail of all checkpoint changes
CREATE OR REPLACE FUNCTION core.observer_checkpoints_audit()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO core.observer_checkpoints_history (listener_id, last_processed_id, batch_size, event_count, reason)
    VALUES (NEW.listener_id, NEW.last_processed_id, NEW.batch_size, NEW.event_count, 'auto_update');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- `DROP TRIGGER` and `CREATE TRIGGER` lock the table even when they change nothing, and
-- every replica runs this at boot, so the trigger is guarded by its definition in the
-- catalog: its function, its timing and events (`tgtype`: ROW 1 | INSERT 4 | UPDATE 16; AFTER is the absence of BEFORE 2), and that it is
-- enabled with no condition, columns or arguments. Any difference recreates it. The
-- catalog's fields, not `pg_get_triggerdef`, whose text qualifies names by the session's
-- search_path.
DO $checkpoints_audit_trigger$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgrelid = 'core.observer_checkpoints'::regclass AND tgname = 'observer_checkpoints_audit_trigger'
          AND tgfoid = 'core.observer_checkpoints_audit()'::regprocedure AND tgtype = 1 | 4 | 16
          AND tgenabled = 'O' AND tgqual IS NULL AND tgnargs = 0 AND tgattr = ''::int2vector
    ) THEN
        DROP TRIGGER IF EXISTS observer_checkpoints_audit_trigger ON core.observer_checkpoints;
        CREATE TRIGGER observer_checkpoints_audit_trigger
            AFTER INSERT OR UPDATE ON core.observer_checkpoints
            FOR EACH ROW
            EXECUTE FUNCTION core.observer_checkpoints_audit();
    END IF;
END
$checkpoints_audit_trigger$;
