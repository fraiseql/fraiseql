-- Table-backed Arrow view for Order
-- Generated view name: ta_order
-- Purpose: Materialized view storing Arrow-encoded columnar data for Order
-- Refresh strategy: scheduled
-- Used by: Arrow Flight streaming and columnar bulk exports



CREATE TABLE IF NOT EXISTS ta_order (
    -- View metadata
    batch_id BIGSERIAL PRIMARY KEY,
    batch_number INTEGER NOT NULL,

    -- Arrow columnar storage
    -- Each column stores Arrow IPC-encoded RecordBatch for the field
    
    col_ BYTEA NOT NULL DEFAULT ''::bytea,
    

    -- Batch metadata
    row_count INTEGER NOT NULL DEFAULT 0,
    batch_size_bytes BIGINT NOT NULL DEFAULT 0,
    compression CHAR(10) DEFAULT 'none',

    -- Flight metadata
    dictionary_encoded_fields TEXT[] DEFAULT ARRAY[]::TEXT[],
    field_compression_codecs TEXT[] DEFAULT ARRAY[]::TEXT[],

    -- Materialization metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    view_generated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Refresh tracking
    is_stale BOOLEAN DEFAULT false,
    staleness_detected_at TIMESTAMP WITH TIME ZONE,
    last_materialized_row_count BIGINT,

    -- Performance hints
    estimated_decode_time_ms INTEGER,
    check_interval INTERVAL DEFAULT '30 minutes'
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_ta_order_batch_number
    ON ta_order(batch_number DESC);

CREATE INDEX IF NOT EXISTS idx_ta_order_updated_at
    ON ta_order(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_ta_order_is_stale
    ON ta_order(is_stale)
    WHERE is_stale = true;

CREATE INDEX IF NOT EXISTS idx_ta_order_row_count
    ON ta_order(row_count DESC);

-- Comments for documentation
COMMENT ON TABLE ta_order IS
    'Table-backed Arrow view storing Order entities as Arrow IPC RecordBatches for efficient columnar streaming';
COMMENT ON COLUMN ta_order.batch_id IS
    'Unique identifier for this Arrow batch';
COMMENT ON COLUMN ta_order.batch_number IS
    'Sequential batch number for ordering in Flight responses';
COMMENT ON COLUMN ta_order.row_count IS
    'Number of rows encoded in this batch';
COMMENT ON COLUMN ta_order.batch_size_bytes IS
    'Total size in bytes of Arrow-encoded data across all columns';
COMMENT ON COLUMN ta_order.is_stale IS
    'Flag indicating if this view entry needs refresh due to source data changes';
COMMENT ON COLUMN ta_order.compression IS
    'Compression codec used for Arrow buffers (none, snappy, lz4, zstd)';


-- Scheduled batch refresh strategy for Order
-- Purpose: Periodically materialize view data in bulk batches
-- Strategy: Higher latency but lower overhead, suitable for read-heavy workloads

-- Refresh state tracking table


CREATE TABLE IF NOT EXISTS refresh_state_order (
    state_id SERIAL PRIMARY KEY,
    last_refresh_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT EPOCH,
    next_scheduled_refresh TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    refresh_duration_ms INTEGER,
    rows_refreshed INTEGER DEFAULT 0,
    refresh_status VARCHAR(50) DEFAULT 'idle',
    error_message TEXT,
    refresh_schedule TEXT DEFAULT '30 minutes',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Scheduling function
CREATE OR REPLACE FUNCTION schedule_next_refresh_order()
RETURNS VOID AS $$
DECLARE
    v_schedule INTERVAL;
BEGIN
    -- Get the refresh schedule from configuration
    v_schedule := INTERVAL '30 minutes';

    UPDATE refresh_state_order
    SET
        next_scheduled_refresh = CURRENT_TIMESTAMP + v_schedule,
        updated_at = CURRENT_TIMESTAMP
    WHERE state_id = 1;
END;
$$ LANGUAGE plpgsql;

-- Full refresh function
-- Completely refreshes all entries in the table-backed view
CREATE OR REPLACE FUNCTION refresh_ta_order_full()
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
    INSERT INTO refresh_state_order (refresh_status, updated_at)
    VALUES ('in_progress', CURRENT_TIMESTAMP)
    ON CONFLICT DO NOTHING;

    -- Truncate existing batch data
    TRUNCATE TABLE ta_order;

    -- Process entities in batches
    FOR v_processed_ids IN
        SELECT ARRAY_AGG(id) FROM (
            SELECT id FROM table_order
            ORDER BY id ASC
            LIMIT v_batch_size
        ) batch
    LOOP
        v_batch_count := v_batch_count + 1;

        -- Insert batch with Arrow-encoded data
        INSERT INTO ta_order (
            batch_number,
            
            col_,
            
            row_count,
            view_generated_at,
            is_stale
        )
        SELECT
            v_batch_count,
            
            DECODE(ENCODE(::bytea, 'hex'), 'hex'),
            
            COUNT(*),
            CURRENT_TIMESTAMP,
            false
        FROM table_order
        WHERE id = ANY(v_processed_ids)
        GROUP BY 1;

        GET DIAGNOSTICS v_rows_affected = ROW_COUNT;
    END LOOP;

    v_end_time := CURRENT_TIMESTAMP;

    -- Update refresh state
    UPDATE refresh_state_order
    SET
        last_refresh_time = v_start_time,
        rows_refreshed = v_rows_affected,
        refresh_duration_ms = EXTRACT(EPOCH FROM (v_end_time - v_start_time))::INTEGER * 1000,
        refresh_status = 'completed',
        error_message = NULL,
        updated_at = CURRENT_TIMESTAMP
    WHERE state_id = 1;

    -- Schedule next refresh
    PERFORM schedule_next_refresh_order();

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
    UPDATE refresh_state_order
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
CREATE OR REPLACE FUNCTION refresh_ta_order_incremental()
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
    FROM refresh_state_order
    LIMIT 1;

    -- Update entries that changed since last refresh
    UPDATE ta_order tb
    SET
        is_stale = false,
        updated_at = CURRENT_TIMESTAMP,
        view_generated_at = CURRENT_TIMESTAMP
    FROM table_order s
    WHERE s.id = tb.batch_id
        AND s.updated_at > COALESCE(v_last_refresh_time, EPOCH);

    GET DIAGNOSTICS v_rows_affected = ROW_COUNT;
    v_end_time := CURRENT_TIMESTAMP;

    -- Update refresh state
    UPDATE refresh_state_order
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
CREATE OR REPLACE FUNCTION check_refresh_health_order()
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
    FROM refresh_state_order rs
    CROSS JOIN ta_order ta;

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
        'ta_order'::VARCHAR,
        (SELECT last_refresh_time FROM refresh_state_order LIMIT 1),
        v_time_since_refresh,
        v_total_rows,
        v_stale_rows,
        v_health_status;
END;
$$ LANGUAGE plpgsql STABLE;

-- Create refresh log table


CREATE TABLE IF NOT EXISTS refresh_log (
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

COMMENT ON FUNCTION refresh_ta_order_full() IS
    'Perform full refresh of TA_order - materializes all entities from source';
COMMENT ON FUNCTION refresh_ta_order_incremental() IS
    'Perform incremental refresh of TA_order - updates only stale entries';
COMMENT ON FUNCTION check_refresh_health_order() IS
    'Check health status and staleness metrics for TA_order';


-- Monitoring and observability functions for Order views
-- Purpose: Provide insights into view performance, staleness, and health metrics

-- View statistics function
CREATE OR REPLACE FUNCTION get_view_statistics_order()
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
    FROM tv_order;
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

-- Arrow view statistics
CREATE OR REPLACE FUNCTION get_arrow_view_statistics_order()
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
    FROM ta_order;
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

-- Staleness analysis
CREATE OR REPLACE FUNCTION analyze_staleness_order()
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
    FROM tv_order
    WHERE is_stale = true;
END;
$$ LANGUAGE plpgsql STABLE;

-- Query performance analysis
CREATE OR REPLACE FUNCTION analyze_query_performance_order()
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
        pg_total_relation_size('tv_order'::regclass)::NUMERIC / 1024 / 1024,
        pg_indexes_size('tv_order'::regclass)::NUMERIC / 1024 / 1024,
        AVG(pg_column_size(entity_json))::INTEGER
    INTO v_total_rows, v_table_size_mb, v_index_size_mb, v_avg_payload_bytes
    FROM tv_order;

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
CREATE OR REPLACE FUNCTION track_refresh_performance_order()
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
    WHERE view_name = 'tv_order'
    GROUP BY trigger_type;
END;
$$ LANGUAGE plpgsql STABLE;

-- Dashboard view combining all metrics
CREATE OR REPLACE VIEW vw_order_dashboard AS
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
FROM get_view_statistics_order() vs
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
FROM get_arrow_view_statistics_order() avs
UNION ALL
SELECT
    'staleness_analysis' AS metric_category,
    json_build_object(
        'stale_count', sa.stale_entity_count,
        'oldest_age', sa.oldest_stale_entry_age,
        'severity', sa.staleness_severity,
        'entities_over_24h', sa.entities_stale_over_24h
    ) AS metrics
FROM analyze_staleness_order() sa;

-- Health check summary
CREATE OR REPLACE FUNCTION health_check_order_summary()
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
    FROM tv_order;

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
    SELECT MAX(updated_at) INTO v_last_refresh FROM tv_order;
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
    IF (SELECT pg_total_relation_size('tv_order'::regclass) / 1024 / 1024) > 1000 THEN
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

COMMENT ON FUNCTION get_view_statistics_order() IS
    'Get comprehensive statistics for JSON view TV_order';
COMMENT ON FUNCTION get_arrow_view_statistics_order() IS
    'Get comprehensive statistics for Arrow view TA_order';
COMMENT ON FUNCTION analyze_staleness_order() IS
    'Analyze staleness metrics and provide severity assessment';
COMMENT ON FUNCTION analyze_query_performance_order() IS
    'Analyze query performance and provide optimization recommendations';
COMMENT ON FUNCTION track_refresh_performance_order() IS
    'Track refresh operation performance metrics over time';
COMMENT ON FUNCTION health_check_order_summary() IS
    'Comprehensive health check summary for Order views';
COMMENT ON VIEW vw_order_dashboard IS
    'Dashboard view combining all monitoring metrics for Order';
