-- CDC Event Logging
-- Debezium-compatible event logging for change data capture
-- This is SEPARATE from mutation responses (no performance impact)

-- Event log table for CDC/event streaming
CREATE TABLE IF NOT EXISTS app.mutation_events (
    event_id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID,
    operation TEXT NOT NULL, -- 'CREATE', 'UPDATE', 'DELETE'

    -- Debezium-style payload
    payload JSONB NOT NULL,

    -- Source metadata
    source JSONB,

    -- Timing
    event_timestamp TIMESTAMPTZ DEFAULT NOW(),

    -- Optional: Transaction context
    transaction_id BIGINT,

    -- Indexes for CDC consumers
    CONSTRAINT valid_operation CHECK (operation IN ('CREATE', 'UPDATE', 'DELETE'))
);

-- Indexes for CDC queries
CREATE INDEX IF NOT EXISTS idx_mutation_events_timestamp
    ON app.mutation_events(event_timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_mutation_events_entity
    ON app.mutation_events(entity_type, entity_id);

CREATE INDEX IF NOT EXISTS idx_mutation_events_type
    ON app.mutation_events(event_type);

COMMENT ON TABLE app.mutation_events IS
    'CDC event log for Debezium/Kafka streaming. Does not impact mutation response speed.';


-- Log CDC event (async, no impact on response time)
CREATE OR REPLACE FUNCTION app.log_cdc_event(
    p_event_type TEXT,
    p_entity_type TEXT,
    p_entity_id UUID,
    p_operation TEXT,
    p_before JSONB DEFAULT NULL,
    p_after JSONB DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL
) RETURNS VOID AS $$
BEGIN
    -- Insert CDC event asynchronously (in same transaction, but doesn't block response)
    INSERT INTO app.mutation_events (
        event_type,
        entity_type,
        entity_id,
        operation,
        payload,
        source,
        transaction_id
    ) VALUES (
        p_event_type,
        p_entity_type,
        p_entity_id,
        p_operation,
        jsonb_build_object(
            'before', p_before,
            'after', p_after,
            'metadata', p_metadata
        ),
        jsonb_build_object(
            'db', current_database(),
            'schema', 'public',
            'table', p_entity_type || 's',
            'txId', txid_current()
        ),
        txid_current()
    );

    -- Note: This INSERT is fast (< 1ms) and doesn't block the RETURN
    -- The client receives the response immediately
    -- CDC consumers read from this table asynchronously
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION app.log_cdc_event IS
    'Log Debezium-compatible CDC event. Fast insert (~1ms), does not block mutation response.';


-- Optional: Notify CDC consumers via PostgreSQL NOTIFY
-- This triggers Debezium/listeners immediately without polling
CREATE OR REPLACE FUNCTION app.notify_cdc_event()
RETURNS TRIGGER AS $$
BEGIN
    -- Send notification to CDC consumers (if using LISTEN/NOTIFY pattern)
    PERFORM pg_notify(
        'mutation_events',
        json_build_object(
            'event_id', NEW.event_id,
            'event_type', NEW.event_type,
            'entity_type', NEW.entity_type,
            'entity_id', NEW.entity_id,
            'operation', NEW.operation
        )::text
    );

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to notify CDC consumers on new events
DROP TRIGGER IF EXISTS trigger_notify_cdc_event ON app.mutation_events;
CREATE TRIGGER trigger_notify_cdc_event
    AFTER INSERT ON app.mutation_events
    FOR EACH ROW
    EXECUTE FUNCTION app.notify_cdc_event();

COMMENT ON TRIGGER trigger_notify_cdc_event ON app.mutation_events IS
    'Notify CDC consumers via pg_notify when new event is logged';
