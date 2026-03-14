-- Scheduled batch refresh strategy for {{ entity_name }}
-- Purpose: Periodically materialize view data in bulk batches
-- Strategy: Higher latency but lower overhead, suitable for read-heavy workloads

-- Refresh state tracking table
{% if not if_not_exists %}DROP TABLE IF EXISTS refresh_state_{{ view_name }} CASCADE;{% endif %}

CREATE TABLE {% if if_not_exists %}IF NOT EXISTS{% endif %} refresh_state_{{ view_name }} (
    state_id SERIAL PRIMARY KEY,
    last_refresh_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT EPOCH,
    next_scheduled_refresh TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    refresh_duration_ms INTEGER,
    rows_refreshed INTEGER DEFAULT 0,
    refresh_status VARCHAR(50) DEFAULT 'idle',
    error_message TEXT,
    refresh_schedule TEXT DEFAULT '{{ refresh_interval }}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Scheduling function
CREATE OR REPLACE FUNCTION schedule_next_refresh_{{ view_name }}()
RETURNS VOID AS $$
DECLARE
    v_schedule INTERVAL;
BEGIN
    -- Get the refresh schedule from configuration
    v_schedule := INTERVAL '{{ refresh_interval }}';

    UPDATE refresh_state_{{ view_name }}
    SET
        next_scheduled_refresh = CURRENT_TIMESTAMP + v_schedule,
        updated_at = CURRENT_TIMESTAMP
    WHERE state_id = 1;
END;
$$ LANGUAGE plpgsql;

-- Full refresh function
-- Completely refreshes all entries in the table-backed view
CREATE OR REPLACE FUNCTION refresh_ta_{{ view_name }}_full()
RETURNS TABLE (
    refresh_id UUID,
    rows_refreshed INTEGER,
    refresh_duration_ms INTEGER,
    status TEXT,
    start_time TIMESTAMP WITH TIME ZONE,
    end_time TIMESTAMP WITH TIME ZONE
) AS $$
DECLARE
    v_start_time TIMESTAMP;
    v_end_time TIMESTAMP;
    v_rows_affected INTEGER := 0;
    v_refresh_id UUID;
    v_batch_size INTEGER := 1000;
    v_batch_count INTEGER := 0;
    v_processed_ids INTEGER[] := ARRAY[]::INTEGER[];
BEGIN
    v_start_time := CURRENT_TIMESTAMP;
    v_refresh_id := gen_random_uuid();

    -- Mark refresh as in-progress
    INSERT INTO refresh_state_{{ view_name }} (refresh_status, updated_at)
    VALUES ('in_progress', CURRENT_TIMESTAMP)
    ON CONFLICT DO NOTHING;

    -- Truncate existing batch data
    TRUNCATE TABLE ta_{{ view_name }};

    -- Process entities in batches
    FOR v_processed_ids IN
        SELECT ARRAY_AGG(id) FROM (
            SELECT id FROM {{ source_table_name }}
            ORDER BY id ASC
            LIMIT v_batch_size
        ) batch
    LOOP
        v_batch_count := v_batch_count + 1;

        -- Insert batch with Arrow-encoded data
        INSERT INTO ta_{{ view_name }} (
            batch_number,
            {%- for field in fields %}
            col_{{ field.name }},
            {%- endfor %}
            row_count,
            view_generated_at,
            is_stale
        )
        SELECT
            v_batch_count,
            {%- for field in fields %}
            DECODE(ENCODE({{ field.name }}::bytea, 'hex'), 'hex'),
            {%- endfor %}
            COUNT(*),
            CURRENT_TIMESTAMP,
            false
        FROM {{ source_table_name }}
        WHERE id = ANY(v_processed_ids)
        GROUP BY 1;

        GET DIAGNOSTICS v_rows_affected = ROW_COUNT;
    END LOOP;

    v_end_time := CURRENT_TIMESTAMP;

    -- Update refresh state
    UPDATE refresh_state_{{ view_name }}
    SET
        last_refresh_time = v_start_time,
        rows_refreshed = v_rows_affected,
        refresh_duration_ms = EXTRACT(EPOCH FROM (v_end_time - v_start_time))::INTEGER * 1000,
        refresh_status = 'completed',
        error_message = NULL,
        updated_at = CURRENT_TIMESTAMP
    WHERE state_id = 1;

    -- Schedule next refresh
    PERFORM schedule_next_refresh_{{ view_name }}();

    RETURN QUERY
    SELECT
        v_refresh_id,
        v_rows_affected,
        EXTRACT(EPOCH FROM (v_end_time - v_start_time))::INTEGER * 1000,
        'SUCCESS'::TEXT,
        v_start_time,
        v_end_time;

EXCEPTION WHEN OTHERS THEN
    v_end_time := CURRENT_TIMESTAMP;

    -- Record error
    UPDATE refresh_state_{{ view_name }}
    SET
        refresh_status = 'failed',
        error_message = SQLERRM,
        updated_at = CURRENT_TIMESTAMP
    WHERE state_id = 1;

    RETURN QUERY
    SELECT
        v_refresh_id,
        0,
        EXTRACT(EPOCH FROM (v_end_time - v_start_time))::INTEGER * 1000,
        'FAILED: ' || SQLERRM,
        v_start_time,
        v_end_time;
END;
$$ LANGUAGE plpgsql;

-- Incremental refresh function
-- Refreshes only stale entries since last full refresh
CREATE OR REPLACE FUNCTION refresh_ta_{{ view_name }}_incremental()
RETURNS TABLE (
    rows_refreshed INTEGER,
    refresh_duration_ms INTEGER,
    status TEXT
) AS $$
DECLARE
    v_start_time TIMESTAMP;
    v_end_time TIMESTAMP;
    v_rows_affected INTEGER := 0;
    v_last_refresh_time TIMESTAMP;
BEGIN
    v_start_time := CURRENT_TIMESTAMP;

    -- Get last refresh time
    SELECT last_refresh_time INTO v_last_refresh_time
    FROM refresh_state_{{ view_name }}
    LIMIT 1;

    -- Update entries that changed since last refresh
    UPDATE ta_{{ view_name }} tb
    SET
        is_stale = false,
        updated_at = CURRENT_TIMESTAMP,
        view_generated_at = CURRENT_TIMESTAMP
    FROM {{ source_table_name }} s
    WHERE s.id = tb.batch_id
        AND s.updated_at > COALESCE(v_last_refresh_time, EPOCH);

    GET DIAGNOSTICS v_rows_affected = ROW_COUNT;
    v_end_time := CURRENT_TIMESTAMP;

    -- Update refresh state
    UPDATE refresh_state_{{ view_name }}
    SET
        last_refresh_time = v_start_time,
        rows_refreshed = v_rows_affected,
        refresh_duration_ms = EXTRACT(EPOCH FROM (v_end_time - v_start_time))::INTEGER * 1000,
        refresh_status = 'completed',
        updated_at = CURRENT_TIMESTAMP
    WHERE state_id = 1;

    RETURN QUERY
    SELECT
        v_rows_affected,
        EXTRACT(EPOCH FROM (v_end_time - v_start_time))::INTEGER * 1000,
        'SUCCESS'::TEXT;
END;
$$ LANGUAGE plpgsql;

-- Refresh health check
CREATE OR REPLACE FUNCTION check_refresh_health_{{ view_name }}()
RETURNS TABLE (
    view_name VARCHAR,
    last_refresh TIMESTAMP WITH TIME ZONE,
    time_since_refresh INTERVAL,
    total_rows INTEGER,
    stale_rows INTEGER,
    health_status VARCHAR
) AS $$
DECLARE
    v_time_since_refresh INTERVAL;
    v_total_rows INTEGER;
    v_stale_rows INTEGER;
    v_health_status VARCHAR;
BEGIN
    -- Gather statistics
    SELECT
        rs.last_refresh_time,
        CURRENT_TIMESTAMP - rs.last_refresh_time,
        COUNT(*),
        COUNT(*) FILTER (WHERE ta.is_stale = true)
    INTO
        STRICT v_time_since_refresh,
        v_total_rows,
        v_stale_rows
    FROM refresh_state_{{ view_name }} rs
    CROSS JOIN ta_{{ view_name }} ta;

    -- Determine health status
    IF v_stale_rows = 0 THEN
        v_health_status := 'HEALTHY';
    ELSIF v_stale_rows < v_total_rows * 0.1 THEN
        v_health_status := 'WARNING';
    ELSE
        v_health_status := 'CRITICAL';
    END IF;

    RETURN QUERY
    SELECT
        'ta_{{ view_name }}'::VARCHAR,
        (SELECT last_refresh_time FROM refresh_state_{{ view_name }} LIMIT 1),
        v_time_since_refresh,
        v_total_rows,
        v_stale_rows,
        v_health_status;
END;
$$ LANGUAGE plpgsql STABLE;

-- Create refresh log table
{% if not if_not_exists %}DROP TABLE IF EXISTS refresh_log CASCADE;{% endif %}

CREATE TABLE {% if if_not_exists %}IF NOT EXISTS{% endif %} refresh_log (
    log_id BIGSERIAL PRIMARY KEY,
    view_name VARCHAR(255) NOT NULL,
    entity_id INTEGER,
    trigger_type VARCHAR(50),
    refresh_type VARCHAR(50),
    affected_rows INTEGER,
    duration_ms INTEGER,
    status VARCHAR(50) DEFAULT 'pending',
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_refresh_log_view_name
    ON refresh_log(view_name);

CREATE INDEX IF NOT EXISTS idx_refresh_log_created_at
    ON refresh_log(created_at DESC);

COMMENT ON FUNCTION refresh_ta_{{ view_name }}_full() IS
    'Perform full refresh of TA_{{ view_name }} - materializes all entities from source';
COMMENT ON FUNCTION refresh_ta_{{ view_name }}_incremental() IS
    'Perform incremental refresh of TA_{{ view_name }} - updates only stale entries';
COMMENT ON FUNCTION check_refresh_health_{{ view_name }}() IS
    'Check health status and staleness metrics for TA_{{ view_name }}';
