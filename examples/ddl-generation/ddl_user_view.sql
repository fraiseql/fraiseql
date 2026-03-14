-- Table-backed JSON view for User
-- Generated view name: tv_user
-- Purpose: Materialized view storing JSON representations of User for efficient retrieval
-- Refresh strategy: trigger-based



CREATE TABLE IF NOT EXISTS tv_user (
    -- View metadata
    view_id BIGSERIAL PRIMARY KEY,
    entity_id INTEGER NOT NULL UNIQUE,

    -- Payload storage
    entity_json JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Composition tracking (for nested relationship views)
    composition_ids TEXT[] DEFAULT ARRAY[]::TEXT[],

    -- Materialization metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    view_generated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Data quality tracking
    is_stale BOOLEAN DEFAULT false,
    staleness_detected_at TIMESTAMP WITH TIME ZONE,

    -- Indexing hints
    check_interval INTERVAL DEFAULT '1 hour'
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_tv_user_entity_id
    ON tv_user(entity_id);

CREATE INDEX IF NOT EXISTS idx_tv_user_updated_at
    ON tv_user(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_tv_user_is_stale
    ON tv_user(is_stale)
    WHERE is_stale = true;

-- JSONB index for efficient JSON queries
CREATE INDEX IF NOT EXISTS idx_tv_user_entity_json_gin
    ON tv_user USING GIN(entity_json);

-- Composition tracking index
CREATE INDEX IF NOT EXISTS idx_tv_user_composition_ids
    ON tv_user USING GIN(composition_ids);

-- Comments for documentation
COMMENT ON TABLE tv_user IS
    'Table-backed view storing User entities as JSONB for fast retrieval';
COMMENT ON COLUMN tv_user.entity_id IS
    'Reference to the original entity ID in the source table';
COMMENT ON COLUMN tv_user.entity_json IS
    'Complete JSON representation of the entity with all scalar and relationship fields';
COMMENT ON COLUMN tv_user.is_stale IS
    'Flag indicating if this view entry needs refresh due to source data changes';
COMMENT ON COLUMN tv_user.composition_ids IS
    'IDs of composed views that include this entity';


-- Trigger-based refresh strategy for User
-- Purpose: Keep materialized views up-to-date by capturing source changes
-- Strategy: Low-latency update, fires on every source table modification

-- Source table trigger function
-- Marks view entries as stale when source data changes
CREATE OR REPLACE FUNCTION refresh_tv_user_on_change()
RETURNS TRIGGER AS $$
DECLARE
    v_entity_id INTEGER;
    v_affected_count INTEGER := 0;
BEGIN
    -- Extract entity ID from the trigger context
    v_entity_id := COALESCE(NEW.id, OLD.id);

    -- Mark the view entry as stale
    UPDATE tv_user
    SET
        is_stale = true,
        staleness_detected_at = CURRENT_TIMESTAMP
    WHERE
        entity_id = v_entity_id;

    GET DIAGNOSTICS v_affected_count = ROW_COUNT;

    -- Log the refresh request for monitoring
    INSERT INTO refresh_log (view_name, entity_id, trigger_type, affected_rows)
    VALUES ('tv_user', v_entity_id, TG_OP, v_affected_count)
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
        WHERE tgname = 'trig_refresh_tv_user'
    ) THEN
        CREATE TRIGGER trig_refresh_tv_user
        AFTER INSERT OR UPDATE OR DELETE ON table_user
        FOR EACH ROW
        EXECUTE FUNCTION refresh_tv_user_on_change();
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Immediate refresh function
-- Called to update a specific view entry synchronously
CREATE OR REPLACE FUNCTION refresh_tv_user_entry(
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

            '',
            {%- if not loop.last %},{% endif %}

        )
        FROM table_user
        WHERE id = p_entity_id
    );

    IF v_json_result IS NOT NULL THEN
        v_refresh_ts := CURRENT_TIMESTAMP;

        -- Insert or update the view entry
        INSERT INTO tv_user (entity_id, entity_json, view_generated_at, is_stale)
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
CREATE OR REPLACE FUNCTION refresh_tv_user_batch(
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

                '',
                {%- if not loop.last %},{% endif %}

            ) AS entity_json
        FROM table_user
        WHERE id = ANY(p_entity_ids)
    )
    INSERT INTO tv_user (entity_id, entity_json, view_generated_at, is_stale)
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
CREATE OR REPLACE FUNCTION get_stale_tv_user_entries(
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
    FROM tv_user tvf
    WHERE tvf.is_stale = true
    ORDER BY tvf.staleness_detected_at ASC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION refresh_tv_user_on_change() IS
    'Trigger function marking TV_user entries as stale when source changes';
COMMENT ON FUNCTION refresh_tv_user_entry(INTEGER) IS
    'Synchronously refresh a single User entry in TV_user';
COMMENT ON FUNCTION refresh_tv_user_batch(INTEGER[]) IS
    'Batch refresh multiple User entries in TV_user efficiently';
COMMENT ON FUNCTION get_stale_tv_user_entries(INTEGER) IS
    'Returns stale entries in TV_user that need refresh';


-- Monitoring and observability functions for User views
-- Purpose: Provide insights into view performance, staleness, and health metrics

-- View statistics function
CREATE OR REPLACE FUNCTION get_view_statistics_user()
RETURNS TABLE (
    view_type VARCHAR,
    total_entities INTEGER,
    total_size_mb NUMERIC,
    json_payload_avg_bytes INTEGER,
    materialized_entries INTEGER,
    stale_entries INTEGER,
    stale_percentage NUMERIC,
    last_update TIMESTAMP WITH TIME ZONE,
    avg_update_interval INTERVAL,
    data_quality_score NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        'JSON'::VARCHAR,
        COUNT(*)::INTEGER,
        (SUM(pg_column_size(entity_json))::NUMERIC / 1024 / 1024)::NUMERIC,
        AVG(pg_column_size(entity_json))::INTEGER,
        COUNT(*)::INTEGER,
        COUNT(*) FILTER (WHERE is_stale)::INTEGER,
        (COUNT(*) FILTER (WHERE is_stale)::NUMERIC / NULLIF(COUNT(*), 0) * 100)::NUMERIC,
        MAX(updated_at),
        AGE(MAX(updated_at), MIN(updated_at)) / NULLIF(COUNT(*) - 1, 0),
        ROUND(100.0 * (1.0 - COUNT(*) FILTER (WHERE is_stale)::NUMERIC / NULLIF(COUNT(*), 0)), 2)::NUMERIC
    FROM tv_user;
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

-- Arrow view statistics
CREATE OR REPLACE FUNCTION get_arrow_view_statistics_user()
RETURNS TABLE (
    view_type VARCHAR,
    total_batches INTEGER,
    total_rows INTEGER,
    total_size_mb NUMERIC,
    avg_batch_size_kb NUMERIC,
    compression_codec VARCHAR,
    materialized_batches INTEGER,
    stale_batches INTEGER,
    avg_decode_time_ms NUMERIC,
    last_materialization TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        'ARROW'::VARCHAR,
        COUNT(*)::INTEGER,
        SUM(row_count)::INTEGER,
        (SUM(batch_size_bytes)::NUMERIC / 1024 / 1024)::NUMERIC,
        (AVG(batch_size_bytes)::NUMERIC / 1024)::NUMERIC,
        COALESCE(compression, 'none'),
        COUNT(*),
        COUNT(*) FILTER (WHERE is_stale)::INTEGER,
        AVG(estimated_decode_time_ms)::NUMERIC,
        MAX(view_generated_at)
    FROM ta_user;
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

-- Staleness analysis
CREATE OR REPLACE FUNCTION analyze_staleness_user()
RETURNS TABLE (
    stale_entity_count INTEGER,
    oldest_stale_entry_age INTERVAL,
    newest_stale_entry_age INTERVAL,
    avg_staleness_duration INTERVAL,
    entities_stale_over_1h INTEGER,
    entities_stale_over_24h INTEGER,
    staleness_severity VARCHAR
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        COUNT(*)::INTEGER,
        MAX(CURRENT_TIMESTAMP - staleness_detected_at),
        MIN(CURRENT_TIMESTAMP - staleness_detected_at),
        AVG(CURRENT_TIMESTAMP - staleness_detected_at),
        COUNT(*) FILTER (WHERE CURRENT_TIMESTAMP - staleness_detected_at > '1 hour'::INTERVAL)::INTEGER,
        COUNT(*) FILTER (WHERE CURRENT_TIMESTAMP - staleness_detected_at > '24 hours'::INTERVAL)::INTEGER,
        CASE
            WHEN COUNT(*) = 0 THEN 'HEALTHY'
            WHEN COUNT(*) < 10 THEN 'LOW'
            WHEN COUNT(*) < 100 THEN 'MEDIUM'
            ELSE 'HIGH'
        END::VARCHAR
    FROM tv_user
    WHERE is_stale = true;
END;
$$ LANGUAGE plpgsql STABLE;

-- Query performance analysis
CREATE OR REPLACE FUNCTION analyze_query_performance_user()
RETURNS TABLE (
    metric_name VARCHAR,
    metric_value NUMERIC,
    unit VARCHAR,
    recommendation TEXT
) AS $$
DECLARE
    v_total_rows INTEGER;
    v_table_size_mb NUMERIC;
    v_index_size_mb NUMERIC;
    v_avg_payload_bytes INTEGER;
BEGIN
    -- Get statistics
    SELECT
        COUNT(*)::INTEGER,
        pg_total_relation_size('tv_user'::regclass)::NUMERIC / 1024 / 1024,
        pg_indexes_size('tv_user'::regclass)::NUMERIC / 1024 / 1024,
        AVG(pg_column_size(entity_json))::INTEGER
    INTO v_total_rows, v_table_size_mb, v_index_size_mb, v_avg_payload_bytes
    FROM tv_user;

    -- Return metrics
    RETURN QUERY
    SELECT 'Total Entities'::VARCHAR, v_total_rows::NUMERIC, 'rows',
        CASE WHEN v_total_rows > 100000 THEN 'Consider partitioning' ELSE 'OK' END;

    RETURN QUERY
    SELECT 'Table Size'::VARCHAR, v_table_size_mb::NUMERIC, 'MB',
        CASE WHEN v_table_size_mb > 500 THEN 'Large table - monitor performance' ELSE 'OK' END;

    RETURN QUERY
    SELECT 'Avg Payload'::VARCHAR, v_avg_payload_bytes::NUMERIC, 'bytes',
        CASE WHEN v_avg_payload_bytes > 10000 THEN 'Large payloads - consider splitting' ELSE 'OK' END;

    RETURN QUERY
    SELECT 'Index Size Ratio'::VARCHAR,
        ROUND((v_index_size_mb / NULLIF(v_table_size_mb, 0) * 100)::NUMERIC, 2),
        '%',
        CASE WHEN v_index_size_mb / NULLIF(v_table_size_mb, 0) > 0.5 THEN 'Heavy indexing - review index usage' ELSE 'OK' END;
END;
$$ LANGUAGE plpgsql STABLE;

-- Refresh performance tracking
CREATE OR REPLACE FUNCTION track_refresh_performance_user()
RETURNS TABLE (
    refresh_operation VARCHAR,
    avg_duration_ms INTEGER,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,
    execution_count INTEGER,
    success_rate NUMERIC,
    last_execution TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        COALESCE(trigger_type, 'unknown'),
        AVG(duration_ms)::INTEGER,
        MIN(duration_ms)::INTEGER,
        MAX(duration_ms)::INTEGER,
        COUNT(*)::INTEGER,
        ROUND(100.0 * COUNT(*) FILTER (WHERE status = 'completed')::NUMERIC /
              NULLIF(COUNT(*), 0), 2)::NUMERIC,
        MAX(created_at)
    FROM refresh_log
    WHERE view_name = 'tv_user'
    GROUP BY trigger_type;
END;
$$ LANGUAGE plpgsql STABLE;

-- Dashboard view combining all metrics
CREATE OR REPLACE VIEW vw_user_dashboard AS
SELECT
    'view_statistics' AS metric_category,
    json_build_object(
        'view_type', 'JSON',
        'total_entities', vs.total_entities,
        'total_size_mb', vs.total_size_mb,
        'stale_percentage', vs.stale_percentage,
        'data_quality_score', vs.data_quality_score,
        'last_update', vs.last_update
    ) AS metrics
FROM get_view_statistics_user() vs
UNION ALL
SELECT
    'arrow_statistics' AS metric_category,
    json_build_object(
        'view_type', 'ARROW',
        'total_batches', avs.total_batches,
        'total_rows', avs.total_rows,
        'total_size_mb', avs.total_size_mb,
        'compression', avs.compression_codec,
        'last_materialization', avs.last_materialization
    ) AS metrics
FROM get_arrow_view_statistics_user() avs
UNION ALL
SELECT
    'staleness_analysis' AS metric_category,
    json_build_object(
        'stale_count', sa.stale_entity_count,
        'oldest_age', sa.oldest_stale_entry_age,
        'severity', sa.staleness_severity,
        'entities_over_24h', sa.entities_stale_over_24h
    ) AS metrics
FROM analyze_staleness_user() sa;

-- Health check summary
CREATE OR REPLACE FUNCTION health_check_user_summary()
RETURNS TABLE (
    component VARCHAR,
    status VARCHAR,
    message TEXT,
    action_required BOOLEAN
) AS $$
DECLARE
    v_stale_count INTEGER;
    v_total_count INTEGER;
    v_last_refresh TIMESTAMP;
    v_days_since_refresh NUMERIC;
BEGIN
    -- Check staleness
    SELECT COUNT(*), COUNT(*) FILTER (WHERE is_stale = true)
    INTO v_total_count, v_stale_count
    FROM tv_user;

    IF v_stale_count > v_total_count * 0.1 THEN
        RETURN QUERY SELECT
            'Staleness'::VARCHAR,
            'WARNING'::VARCHAR,
            'More than 10% of entries are stale',
            true;
    ELSE
        RETURN QUERY SELECT
            'Staleness'::VARCHAR,
            'OK'::VARCHAR,
            'Staleness within acceptable threshold',
            false;
    END IF;

    -- Check refresh frequency
    SELECT MAX(updated_at) INTO v_last_refresh FROM tv_user;
    v_days_since_refresh := EXTRACT(DAY FROM (CURRENT_TIMESTAMP - v_last_refresh));

    IF v_days_since_refresh > 7 THEN
        RETURN QUERY SELECT
            'Refresh Frequency'::VARCHAR,
            'WARNING'::VARCHAR,
            'No updates in ' || v_days_since_refresh::INTEGER || ' days',
            true;
    ELSE
        RETURN QUERY SELECT
            'Refresh Frequency'::VARCHAR,
            'OK'::VARCHAR,
            'Regular updates detected',
            false;
    END IF;

    -- Check table size
    IF (SELECT pg_total_relation_size('tv_user'::regclass) / 1024 / 1024) > 1000 THEN
        RETURN QUERY SELECT
            'Table Size'::VARCHAR,
            'WARNING'::VARCHAR,
            'Table exceeds 1GB - consider archiving old data',
            true;
    ELSE
        RETURN QUERY SELECT
            'Table Size'::VARCHAR,
            'OK'::VARCHAR,
            'Table size healthy',
            false;
    END IF;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION get_view_statistics_user() IS
    'Get comprehensive statistics for JSON view TV_user';
COMMENT ON FUNCTION get_arrow_view_statistics_user() IS
    'Get comprehensive statistics for Arrow view TA_user';
COMMENT ON FUNCTION analyze_staleness_user() IS
    'Analyze staleness metrics and provide severity assessment';
COMMENT ON FUNCTION analyze_query_performance_user() IS
    'Analyze query performance and provide optimization recommendations';
COMMENT ON FUNCTION track_refresh_performance_user() IS
    'Track refresh operation performance metrics over time';
COMMENT ON FUNCTION health_check_user_summary() IS
    'Comprehensive health check summary for User views';
COMMENT ON VIEW vw_user_dashboard IS
    'Dashboard view combining all monitoring metrics for User';
