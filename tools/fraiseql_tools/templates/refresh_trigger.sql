-- Trigger-based refresh strategy for {{ entity_name }}
-- Purpose: Keep materialized views up-to-date by capturing source changes
-- Strategy: Low-latency update, fires on every source table modification

-- Source table trigger function
-- Marks view entries as stale when source data changes
CREATE OR REPLACE FUNCTION refresh_tv_{{ view_name }}_on_change()
RETURNS TRIGGER AS $$
DECLARE
    v_entity_id INTEGER;
    v_affected_count INTEGER := 0;
BEGIN
    -- Extract entity ID from the trigger context
    v_entity_id := COALESCE(NEW.id, OLD.id);

    -- Mark the view entry as stale
    UPDATE tv_{{ view_name }}
    SET
        is_stale = true,
        staleness_detected_at = CURRENT_TIMESTAMP
    WHERE
        entity_id = v_entity_id;

    GET DIAGNOSTICS v_affected_count = ROW_COUNT;

    -- Log the refresh request for monitoring
    INSERT INTO refresh_log (view_name, entity_id, trigger_type, affected_rows)
    VALUES ('tv_{{ view_name }}', v_entity_id, TG_OP, v_affected_count)
    ON CONFLICT DO NOTHING;

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Attach trigger to source table
-- This trigger fires AFTER INSERT/UPDATE/DELETE on the source table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgname = 'trig_refresh_tv_{{ view_name }}'
    ) THEN
        CREATE TRIGGER trig_refresh_tv_{{ view_name }}
        AFTER INSERT OR UPDATE OR DELETE ON {{ source_table_name }}
        FOR EACH ROW
        EXECUTE FUNCTION refresh_tv_{{ view_name }}_on_change();
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Immediate refresh function
-- Called to update a specific view entry synchronously
CREATE OR REPLACE FUNCTION refresh_tv_{{ view_name }}_entry(
    p_entity_id INTEGER
)
RETURNS TABLE (
    entity_id INTEGER,
    entity_json JSONB,
    refresh_status TEXT
) AS $$
DECLARE
    v_json_result JSONB;
    v_refresh_ts TIMESTAMP;
BEGIN
    -- Construct the entity JSON from source table
    v_json_result := (
        SELECT jsonb_build_object(
            {%- for field in fields %}
            '{{ field.name }}', {{ field.name }}
            {%- if not loop.last %},{% endif %}
            {%- endfor %}
        )
        FROM {{ source_table_name }}
        WHERE id = p_entity_id
    );

    IF v_json_result IS NOT NULL THEN
        v_refresh_ts := CURRENT_TIMESTAMP;

        -- Insert or update the view entry
        INSERT INTO tv_{{ view_name }} (entity_id, entity_json, view_generated_at, is_stale)
        VALUES (p_entity_id, v_json_result, v_refresh_ts, false)
        ON CONFLICT (entity_id) DO UPDATE SET
            entity_json = EXCLUDED.entity_json,
            view_generated_at = v_refresh_ts,
            is_stale = false,
            updated_at = CURRENT_TIMESTAMP;

        RETURN QUERY
        SELECT
            p_entity_id,
            v_json_result,
            'REFRESHED'::TEXT;
    ELSE
        -- Entity not found in source table
        RETURN QUERY
        SELECT
            p_entity_id,
            '{}'::JSONB,
            'NOT_FOUND'::TEXT;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Batch refresh function
-- Efficiently refreshes multiple entries in a single operation
CREATE OR REPLACE FUNCTION refresh_tv_{{ view_name }}_batch(
    p_entity_ids INTEGER[]
)
RETURNS TABLE (
    entity_id INTEGER,
    refresh_status TEXT,
    rows_affected INTEGER
) AS $$
DECLARE
    v_rows_affected INTEGER := 0;
BEGIN
    WITH source_data AS (
        SELECT
            id AS entity_id,
            jsonb_build_object(
                {%- for field in fields %}
                '{{ field.name }}', {{ field.name }}
                {%- if not loop.last %},{% endif %}
                {%- endfor %}
            ) AS entity_json
        FROM {{ source_table_name }}
        WHERE id = ANY(p_entity_ids)
    )
    INSERT INTO tv_{{ view_name }} (entity_id, entity_json, view_generated_at, is_stale)
    SELECT
        sd.entity_id,
        sd.entity_json,
        CURRENT_TIMESTAMP,
        false
    FROM source_data sd
    ON CONFLICT (entity_id) DO UPDATE SET
        entity_json = EXCLUDED.entity_json,
        view_generated_at = CURRENT_TIMESTAMP,
        is_stale = false,
        updated_at = CURRENT_TIMESTAMP;

    GET DIAGNOSTICS v_rows_affected = ROW_COUNT;

    RETURN QUERY
    SELECT
        UNNEST(p_entity_ids),
        'REFRESHED'::TEXT,
        v_rows_affected;
END;
$$ LANGUAGE plpgsql;

-- Health check function
-- Returns stale entries that need refresh
CREATE OR REPLACE FUNCTION get_stale_tv_{{ view_name }}_entries(
    p_limit INTEGER DEFAULT 100
)
RETURNS TABLE (
    entity_id INTEGER,
    stale_since TIMESTAMP WITH TIME ZONE,
    stale_duration INTERVAL
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        tvf.entity_id,
        tvf.staleness_detected_at,
        CURRENT_TIMESTAMP - tvf.staleness_detected_at
    FROM tv_{{ view_name }} tvf
    WHERE tvf.is_stale = true
    ORDER BY tvf.staleness_detected_at ASC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION refresh_tv_{{ view_name }}_on_change() IS
    'Trigger function marking TV_{{ view_name }} entries as stale when source changes';
COMMENT ON FUNCTION refresh_tv_{{ view_name }}_entry(INTEGER) IS
    'Synchronously refresh a single {{ entity_name }} entry in TV_{{ view_name }}';
COMMENT ON FUNCTION refresh_tv_{{ view_name }}_batch(INTEGER[]) IS
    'Batch refresh multiple {{ entity_name }} entries in TV_{{ view_name }} efficiently';
COMMENT ON FUNCTION get_stale_tv_{{ view_name }}_entries(INTEGER) IS
    'Returns stale entries in TV_{{ view_name }} that need refresh';
