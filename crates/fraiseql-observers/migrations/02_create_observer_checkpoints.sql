-- FraiseQL Observer System - Persistent Checkpoints
-- Stores listener checkpoint state for recovery on restart.
--
-- Trinity Pattern:
--   pk_{entity} BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY
--   id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE
--   identifier  TEXT NOT NULL UNIQUE  (human-readable key)

CREATE TABLE IF NOT EXISTS tb_observer_checkpoint (
    -- Trinity: internal PK
    pk_observer_checkpoint BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Trinity: external UUID
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Trinity: human-readable identifier (e.g., listener name)
    identifier TEXT NOT NULL UNIQUE,

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
CREATE INDEX IF NOT EXISTS idx_tb_observer_checkpoint_updated_at
    ON tb_observer_checkpoint(updated_at DESC);

-- Audit trail: Track all checkpoint updates
CREATE TABLE IF NOT EXISTS tb_observer_checkpoint_history (
    -- Trinity: internal PK
    pk_observer_checkpoint_history BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Trinity: external UUID
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,

    -- Trinity FK to parent checkpoint
    fk_observer_checkpoint BIGINT NOT NULL REFERENCES tb_observer_checkpoint(pk_observer_checkpoint),

    last_processed_id BIGINT NOT NULL,
    batch_size INT NOT NULL,
    event_count INT NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    reason VARCHAR(255),

    CONSTRAINT valid_history_ids CHECK (last_processed_id >= 0)
);

CREATE INDEX IF NOT EXISTS idx_tb_observer_checkpoint_history_fk
    ON tb_observer_checkpoint_history(fk_observer_checkpoint);

CREATE INDEX IF NOT EXISTS idx_tb_observer_checkpoint_history_updated_at
    ON tb_observer_checkpoint_history(updated_at DESC);

-- Trigger to auto-populate history (optional, but useful for debugging)
-- This creates an audit trail of all checkpoint changes
CREATE OR REPLACE FUNCTION observer_checkpoints_audit()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO tb_observer_checkpoint_history (
        fk_observer_checkpoint, last_processed_id, batch_size, event_count, reason
    )
    VALUES (NEW.pk_observer_checkpoint, NEW.last_processed_id, NEW.batch_size, NEW.event_count, 'auto_update');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger only if it doesn't exist
DROP TRIGGER IF EXISTS observer_checkpoints_audit_trigger ON tb_observer_checkpoint;
CREATE TRIGGER observer_checkpoints_audit_trigger
AFTER INSERT OR UPDATE ON tb_observer_checkpoint
FOR EACH ROW
EXECUTE FUNCTION observer_checkpoints_audit();
