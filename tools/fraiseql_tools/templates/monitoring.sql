-- Monitoring and observability functions for {{ entity_name }} views
-- Purpose: Provide insights into view performance, staleness, and health metrics

-- View statistics function
CREATE OR REPLACE FUNCTION get_view_statistics_{{ view_name }}()
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
    FROM tv_{{ view_name }};
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

-- Arrow view statistics
CREATE OR REPLACE FUNCTION get_arrow_view_statistics_{{ view_name }}()
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
    FROM ta_{{ view_name }};
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

-- Staleness analysis
CREATE OR REPLACE FUNCTION analyze_staleness_{{ view_name }}()
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
    FROM tv_{{ view_name }}
    WHERE is_stale = true;
END;
$$ LANGUAGE plpgsql STABLE;

-- Query performance analysis
CREATE OR REPLACE FUNCTION analyze_query_performance_{{ view_name }}()
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
        pg_total_relation_size('tv_{{ view_name }}'::regclass)::NUMERIC / 1024 / 1024,
        pg_indexes_size('tv_{{ view_name }}'::regclass)::NUMERIC / 1024 / 1024,
        AVG(pg_column_size(entity_json))::INTEGER
    INTO v_total_rows, v_table_size_mb, v_index_size_mb, v_avg_payload_bytes
    FROM tv_{{ view_name }};

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
CREATE OR REPLACE FUNCTION track_refresh_performance_{{ view_name }}()
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
    WHERE view_name = 'tv_{{ view_name }}'
    GROUP BY trigger_type;
END;
$$ LANGUAGE plpgsql STABLE;

-- Dashboard view combining all metrics
CREATE OR REPLACE VIEW vw_{{ view_name }}_dashboard AS
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
FROM get_view_statistics_{{ view_name }}() vs
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
FROM get_arrow_view_statistics_{{ view_name }}() avs
UNION ALL
SELECT
    'staleness_analysis' AS metric_category,
    json_build_object(
        'stale_count', sa.stale_entity_count,
        'oldest_age', sa.oldest_stale_entry_age,
        'severity', sa.staleness_severity,
        'entities_over_24h', sa.entities_stale_over_24h
    ) AS metrics
FROM analyze_staleness_{{ view_name }}() sa;

-- Health check summary
CREATE OR REPLACE FUNCTION health_check_{{ view_name }}_summary()
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
    FROM tv_{{ view_name }};

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
    SELECT MAX(updated_at) INTO v_last_refresh FROM tv_{{ view_name }};
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
    IF (SELECT pg_total_relation_size('tv_{{ view_name }}'::regclass) / 1024 / 1024) > 1000 THEN
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

COMMENT ON FUNCTION get_view_statistics_{{ view_name }}() IS
    'Get comprehensive statistics for JSON view TV_{{ view_name }}';
COMMENT ON FUNCTION get_arrow_view_statistics_{{ view_name }}() IS
    'Get comprehensive statistics for Arrow view TA_{{ view_name }}';
COMMENT ON FUNCTION analyze_staleness_{{ view_name }}() IS
    'Analyze staleness metrics and provide severity assessment';
COMMENT ON FUNCTION analyze_query_performance_{{ view_name }}() IS
    'Analyze query performance and provide optimization recommendations';
COMMENT ON FUNCTION track_refresh_performance_{{ view_name }}() IS
    'Track refresh operation performance metrics over time';
COMMENT ON FUNCTION health_check_{{ view_name }}_summary() IS
    'Comprehensive health check summary for {{ entity_name }} views';
COMMENT ON VIEW vw_{{ view_name }}_dashboard IS
    'Dashboard view combining all monitoring metrics for {{ entity_name }}';
