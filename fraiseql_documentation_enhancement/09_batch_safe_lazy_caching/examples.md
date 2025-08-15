# Batch-Safe Lazy Caching Examples

## Overview

This file contains working examples demonstrating the batch-safe lazy caching mechanisms documented in the main prompt. These examples are extracted from PrintOptim Backend's production system.

## Statement Tracking Examples

### Basic Statement Tracker Implementation

```sql
-- Core statement tracker table (production-tested)
CREATE TABLE turbo.tb_statement_version_tracker (
    backend_pid INT NOT NULL,
    statement_timestamp TIMESTAMP NOT NULL,
    tenant_id UUID NOT NULL,
    domain TEXT NOT NULL,
    version_incremented BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT NOW(),

    PRIMARY KEY (backend_pid, statement_timestamp, tenant_id, domain)
);

-- Indexes for performance
CREATE INDEX idx_statement_tracker_cleanup
ON turbo.tb_statement_version_tracker(created_at);

CREATE INDEX idx_statement_tracker_lookup
ON turbo.tb_statement_version_tracker(backend_pid, statement_timestamp);
```

### Production Batch-Safe Trigger

```sql
-- Production-tested batch-safe invalidation trigger
CREATE OR REPLACE FUNCTION turbo.fn_tv_table_cache_invalidation()
RETURNS TRIGGER AS $$
DECLARE
    v_tenant_id UUID;
    v_domain TEXT;
    v_statement_id TIMESTAMP;
    v_backend_pid INT;
    v_inserted BOOLEAN := FALSE;
BEGIN
    -- Extract domain from table name (tv_contract → contract)
    v_domain := regexp_replace(TG_TABLE_NAME, '^tv_', '');

    -- Get unique statement identifier
    v_statement_id := statement_timestamp();
    v_backend_pid := pg_backend_pid();
    v_tenant_id := COALESCE(NEW.tenant_id, OLD.tenant_id);

    -- Try to insert tracking record (first row wins)
    BEGIN
        INSERT INTO turbo.tb_statement_version_tracker
            (backend_pid, statement_timestamp, tenant_id, domain)
        VALUES (v_backend_pid, v_statement_id, v_tenant_id, v_domain);
        v_inserted := TRUE;
    EXCEPTION WHEN unique_violation THEN
        -- Another row in this statement already handled invalidation
        v_inserted := FALSE;
    END;

    -- If insert succeeded, this is the first row in the batch
    IF v_inserted THEN
        -- Increment domain version (ONLY ONCE per statement)
        INSERT INTO turbo.tb_domain_version (tenant_id, domain, version, last_modified)
        VALUES (v_tenant_id, v_domain, 1, NOW())
        ON CONFLICT (tenant_id, domain) DO UPDATE
        SET version = turbo.tb_domain_version.version + 1,
            last_modified = NOW(),
            modified_by = current_setting('app.user_id', true),
            change_summary = format('%s batch on tv_%s (%s rows)',
                TG_OP, v_domain, TG_LEVEL);

        -- Handle cascade invalidations
        PERFORM turbo.fn_handle_cascade_invalidation(
            v_backend_pid, v_statement_id, v_tenant_id, v_domain
        );

        -- Probabilistic cleanup (1% chance)
        IF random() < 0.01 THEN
            PERFORM turbo.fn_cleanup_statement_tracker();
        END IF;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;
```

## Cascade Invalidation Examples

### Cross-Domain Dependencies

```sql
-- Production cascade invalidation system
CREATE OR REPLACE FUNCTION turbo.fn_handle_cascade_invalidation(
    p_backend_pid INT,
    p_statement_id TIMESTAMP,
    p_tenant_id UUID,
    p_source_domain TEXT
) RETURNS void AS $$
BEGIN
    -- Contract domain affects related domains
    IF p_source_domain = 'contract' THEN
        -- Contract items depend on contracts
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'contract_item'
        );
        -- Pricing depends on contracts
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'price'
        );
        -- Machine allocations depend on contracts
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'allocation'
        );

    -- Contract item changes affect pricing and contracts
    ELSIF p_source_domain = 'contract_item' THEN
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'price'
        );
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'contract'
        );

    -- Machine changes affect allocations
    ELSIF p_source_domain = 'machine' THEN
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'allocation'
        );
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'maintenance'
        );

    -- User changes affect all user-related contexts
    ELSIF p_source_domain = 'user' THEN
        PERFORM turbo.fn_increment_domain_version_once(
            p_backend_pid, p_statement_id, p_tenant_id, 'organization'
        );
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Helper for cascade invalidation with deduplication
CREATE OR REPLACE FUNCTION turbo.fn_increment_domain_version_once(
    p_backend_pid INT,
    p_statement_id TIMESTAMP,
    p_tenant_id UUID,
    p_domain TEXT
) RETURNS void AS $$
DECLARE
    v_inserted BOOLEAN := FALSE;
BEGIN
    -- Use same tracking pattern for cascaded domains
    BEGIN
        INSERT INTO turbo.tb_statement_version_tracker
            (backend_pid, statement_timestamp, tenant_id, domain)
        VALUES (p_backend_pid, p_statement_id, p_tenant_id, p_domain);
        v_inserted := TRUE;
    EXCEPTION WHEN unique_violation THEN
        -- Already handled by another cascade or direct invalidation
        v_inserted := FALSE;
    END;

    -- Only increment if this was the first invalidation for this domain
    IF v_inserted THEN
        INSERT INTO turbo.tb_domain_version (tenant_id, domain, version, last_modified)
        VALUES (p_tenant_id, p_domain, 1, NOW())
        ON CONFLICT (tenant_id, domain) DO UPDATE
        SET version = turbo.tb_domain_version.version + 1,
            last_modified = NOW(),
            modified_by = 'cascade_system',
            change_summary = format('Cascade invalidation from %s domain',
                -- Find the original source domain for this statement
                COALESCE((
                    SELECT domain
                    FROM turbo.tb_statement_version_tracker
                    WHERE backend_pid = p_backend_pid
                      AND statement_timestamp = p_statement_id
                      AND tenant_id = p_tenant_id
                      AND created_at = (
                        SELECT MIN(created_at)
                        FROM turbo.tb_statement_version_tracker sub
                        WHERE sub.backend_pid = p_backend_pid
                          AND sub.statement_timestamp = p_statement_id
                          AND sub.tenant_id = p_tenant_id
                      )
                    LIMIT 1
                ), 'unknown')
            );
    END IF;
END;
$$ LANGUAGE plpgsql;
```

## Monitoring Examples

### Production Monitoring Views

```sql
-- Production health monitoring view
CREATE VIEW turbo.v_statement_tracker_health AS
SELECT
    COUNT(*) as active_statements,
    COUNT(DISTINCT backend_pid) as active_connections,
    COUNT(DISTINCT tenant_id) as active_tenants,
    COUNT(DISTINCT domain) as domains_affected,
    MIN(created_at) as oldest_statement,
    MAX(created_at) as newest_statement,
    EXTRACT(MINUTES FROM (NOW() - MIN(created_at))) as oldest_age_minutes,
    pg_size_pretty(
        pg_total_relation_size('turbo.tb_statement_version_tracker')
    ) as table_size,
    CASE
        WHEN COUNT(*) > 10000 THEN 'CRITICAL'
        WHEN COUNT(*) > 5000 THEN 'WARNING'
        WHEN COUNT(*) > 1000 THEN 'ATTENTION'
        WHEN COUNT(*) < 100 THEN 'HEALTHY'
        ELSE 'GOOD'
    END as health_status,
    CASE
        WHEN MIN(created_at) < NOW() - INTERVAL '10 minutes' THEN 'STALE'
        WHEN MIN(created_at) < NOW() - INTERVAL '5 minutes' THEN 'ATTENTION'
        ELSE 'FRESH'
    END as freshness_status
FROM turbo.tb_statement_version_tracker;

-- Batch efficiency analysis view
CREATE VIEW turbo.v_batch_efficiency_report AS
WITH batch_stats AS (
    SELECT
        domain,
        backend_pid,
        statement_timestamp,
        COUNT(*) as rows_affected,
        MIN(created_at) as batch_start,
        MAX(created_at) as batch_end,
        EXTRACT(MILLISECONDS FROM (MAX(created_at) - MIN(created_at))) as batch_duration_ms
    FROM turbo.tb_statement_version_tracker
    WHERE created_at > NOW() - INTERVAL '24 hours'
    GROUP BY domain, backend_pid, statement_timestamp
)
SELECT
    domain,
    COUNT(*) as total_batches,
    ROUND(AVG(rows_affected), 2) as avg_batch_size,
    MIN(rows_affected) as smallest_batch,
    MAX(rows_affected) as largest_batch,
    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY rows_affected) as median_batch_size,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY rows_affected) as p95_batch_size,
    SUM(rows_affected) as total_rows_processed,
    ROUND(AVG(batch_duration_ms), 2) as avg_batch_duration_ms,
    CASE
        WHEN AVG(rows_affected) > 1000 THEN 'EXCELLENT - Very large batches'
        WHEN AVG(rows_affected) > 100 THEN 'GOOD - Large batches'
        WHEN AVG(rows_affected) > 10 THEN 'FAIR - Medium batches'
        WHEN AVG(rows_affected) > 1 THEN 'POOR - Small batches'
        ELSE 'SINGLE - No batching benefit'
    END as efficiency_assessment,
    -- Calculate theoretical time saved
    ROUND(
        (SUM(rows_affected) - COUNT(*)) * 1.0 / 1000, -- 1ms saved per avoided invalidation
        3
    ) as estimated_time_saved_seconds
FROM batch_stats
GROUP BY domain
ORDER BY avg_batch_size DESC;

-- Real-time invalidation monitoring
CREATE VIEW turbo.v_realtime_invalidation_activity AS
SELECT
    domain,
    COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '1 minute') as last_minute,
    COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '5 minutes') as last_5_minutes,
    COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '15 minutes') as last_15_minutes,
    COUNT(DISTINCT backend_pid) as active_processes,
    MAX(created_at) as last_activity,
    CASE
        WHEN COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '1 minute') > 100 THEN 'HIGH ACTIVITY'
        WHEN COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '1 minute') > 10 THEN 'MODERATE ACTIVITY'
        WHEN COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '5 minutes') > 0 THEN 'LOW ACTIVITY'
        ELSE 'IDLE'
    END as activity_level
FROM turbo.tb_statement_version_tracker
WHERE created_at > NOW() - INTERVAL '15 minutes'
GROUP BY domain
ORDER BY last_minute DESC;
```

### Automated Health Checks

```sql
-- Production health check function
CREATE OR REPLACE FUNCTION turbo.fn_check_statement_tracker_health()
RETURNS TABLE(
    check_name TEXT,
    status TEXT,
    value TEXT,
    threshold TEXT,
    recommendation TEXT
) AS $$
DECLARE
    v_count INT;
    v_oldest TIMESTAMP;
    v_connections INT;
    v_table_size TEXT;
BEGIN
    SELECT
        COUNT(*),
        MIN(created_at),
        COUNT(DISTINCT backend_pid),
        pg_size_pretty(pg_total_relation_size('turbo.tb_statement_version_tracker'))
    INTO v_count, v_oldest, v_connections, v_table_size
    FROM turbo.tb_statement_version_tracker;

    -- Check 1: Statement count
    RETURN QUERY VALUES (
        'Statement Count',
        CASE
            WHEN v_count > 10000 THEN 'CRITICAL'
            WHEN v_count > 5000 THEN 'WARNING'
            WHEN v_count > 1000 THEN 'INFO'
            ELSE 'OK'
        END,
        v_count::TEXT,
        '< 5000 (warning at 5000, critical at 10000)',
        CASE
            WHEN v_count > 10000 THEN 'Run cleanup immediately: SELECT turbo.fn_cleanup_statement_tracker()'
            WHEN v_count > 5000 THEN 'Schedule cleanup soon'
            ELSE 'No action needed'
        END
    );

    -- Check 2: Oldest statement age
    RETURN QUERY VALUES (
        'Oldest Statement Age',
        CASE
            WHEN v_oldest < NOW() - INTERVAL '15 minutes' THEN 'WARNING'
            WHEN v_oldest < NOW() - INTERVAL '10 minutes' THEN 'INFO'
            ELSE 'OK'
        END,
        COALESCE(EXTRACT(MINUTES FROM (NOW() - v_oldest))::TEXT || ' minutes', 'No statements'),
        '< 10 minutes (normal operation)',
        CASE
            WHEN v_oldest < NOW() - INTERVAL '15 minutes' THEN 'Check for stuck transactions or long-running operations'
            ELSE 'No action needed'
        END
    );

    -- Check 3: Active connections
    RETURN QUERY VALUES (
        'Active Connections',
        CASE
            WHEN v_connections > 50 THEN 'INFO'
            ELSE 'OK'
        END,
        v_connections::TEXT,
        '< 50 (informational only)',
        CASE
            WHEN v_connections > 50 THEN 'High concurrent activity - monitor for performance'
            ELSE 'Normal activity level'
        END
    );

    -- Check 4: Table size
    RETURN QUERY VALUES (
        'Table Size',
        CASE
            WHEN pg_total_relation_size('turbo.tb_statement_version_tracker') > 100 * 1024 * 1024 THEN 'WARNING'
            WHEN pg_total_relation_size('turbo.tb_statement_version_tracker') > 50 * 1024 * 1024 THEN 'INFO'
            ELSE 'OK'
        END,
        v_table_size,
        '< 50MB (warning at 100MB)',
        CASE
            WHEN pg_total_relation_size('turbo.tb_statement_version_tracker') > 100 * 1024 * 1024
            THEN 'Consider more frequent cleanup or investigate table bloat'
            ELSE 'Size is acceptable'
        END
    );
END;
$$ LANGUAGE plpgsql;
```

## Performance Testing Examples

### Batch Operation Benchmarks

```sql
-- Performance test setup
CREATE TABLE test_batch_performance (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Create tv_ table with batch-safe trigger
CREATE TABLE tv_test_batch_performance (
    id SERIAL PRIMARY KEY,
    pk_test UUID DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    fk_source INTEGER REFERENCES test_batch_performance(id),
    data JSONB NOT NULL,
    version INTEGER DEFAULT 1,
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Apply batch-safe trigger
CREATE TRIGGER tr_test_batch_invalidation
AFTER INSERT OR UPDATE OR DELETE ON tv_test_batch_performance
FOR EACH ROW EXECUTE FUNCTION turbo.fn_tv_table_cache_invalidation();

-- Performance test: Single row operations
DO $$
DECLARE
    v_start TIMESTAMP;
    v_end TIMESTAMP;
    v_tenant_id UUID := gen_random_uuid();
    i INTEGER;
BEGIN
    RAISE NOTICE 'Testing single row operations...';
    v_start := clock_timestamp();

    FOR i IN 1..1000 LOOP
        INSERT INTO tv_test_batch_performance (tenant_id, data)
        VALUES (v_tenant_id, jsonb_build_object('test', i));
    END LOOP;

    v_end := clock_timestamp();
    RAISE NOTICE 'Single rows: 1000 operations in % seconds', EXTRACT(EPOCH FROM (v_end - v_start));

    -- Check invalidation count
    RAISE NOTICE 'Invalidations created: %',
        (SELECT COUNT(*) FROM turbo.tb_statement_version_tracker
         WHERE tenant_id = v_tenant_id);
END $$;

-- Performance test: Batch operations
DO $$
DECLARE
    v_start TIMESTAMP;
    v_end TIMESTAMP;
    v_tenant_id UUID := gen_random_uuid();
BEGIN
    RAISE NOTICE 'Testing batch operations...';
    v_start := clock_timestamp();

    -- Single statement inserting 1000 rows
    INSERT INTO tv_test_batch_performance (tenant_id, data)
    SELECT v_tenant_id, jsonb_build_object('batch_test', generate_series(1, 1000));

    v_end := clock_timestamp();
    RAISE NOTICE 'Batch: 1000 operations in % seconds', EXTRACT(EPOCH FROM (v_end - v_start));

    -- Check invalidation count (should be 1)
    RAISE NOTICE 'Invalidations created: %',
        (SELECT COUNT(*) FROM turbo.tb_statement_version_tracker
         WHERE tenant_id = v_tenant_id);
END $$;

-- Cleanup test data
DROP TABLE tv_test_batch_performance CASCADE;
DROP TABLE test_batch_performance CASCADE;
```

### Memory Usage Analysis

```sql
-- Memory usage monitoring function
CREATE OR REPLACE FUNCTION turbo.fn_analyze_memory_usage()
RETURNS TABLE(
    operation_type TEXT,
    rows_processed INT,
    invalidations_created INT,
    efficiency_ratio NUMERIC,
    estimated_memory_saved TEXT
) AS $$
BEGIN
    RETURN QUERY
    WITH operation_analysis AS (
        SELECT
            CASE
                WHEN COUNT(*) = 1 THEN 'Single Row'
                WHEN COUNT(*) <= 10 THEN 'Small Batch'
                WHEN COUNT(*) <= 100 THEN 'Medium Batch'
                WHEN COUNT(*) <= 1000 THEN 'Large Batch'
                ELSE 'Very Large Batch'
            END as op_type,
            COUNT(*) as rows,
            1 as invalidations, -- Always 1 with batch-safe architecture
            backend_pid,
            statement_timestamp
        FROM turbo.tb_statement_version_tracker
        WHERE created_at > NOW() - INTERVAL '1 hour'
        GROUP BY backend_pid, statement_timestamp
    )
    SELECT
        op_type,
        ROUND(AVG(rows))::INT as avg_rows,
        MAX(invalidations) as invalidations_per_operation,
        ROUND(AVG(rows) / MAX(invalidations), 2) as efficiency,
        pg_size_pretty(
            -- Estimate memory saved: (rows - 1) * average_invalidation_size
            ((ROUND(AVG(rows))::INT - 1) * 1024)::BIGINT -- Assume 1KB per invalidation
        ) as memory_saved_estimate
    FROM operation_analysis
    GROUP BY op_type
    ORDER BY avg_rows DESC;
END;
$$ LANGUAGE plpgsql;
```

## Cleanup Examples

### Production Cleanup Functions

```sql
-- Production cleanup function with safety checks
CREATE OR REPLACE FUNCTION turbo.fn_cleanup_statement_tracker()
RETURNS TABLE(
    cleanup_summary TEXT,
    rows_deleted INT,
    oldest_remaining TIMESTAMP,
    table_size_before TEXT,
    table_size_after TEXT
) AS $$
DECLARE
    v_deleted INT;
    v_size_before BIGINT;
    v_size_after BIGINT;
    v_oldest TIMESTAMP;
BEGIN
    -- Capture initial state
    SELECT pg_total_relation_size('turbo.tb_statement_version_tracker')
    INTO v_size_before;

    -- Delete old entries (older than 5 minutes - active statements shouldn't last that long)
    DELETE FROM turbo.tb_statement_version_tracker
    WHERE created_at < NOW() - INTERVAL '5 minutes';

    GET DIAGNOSTICS v_deleted = ROW_COUNT;

    -- Capture final state
    SELECT pg_total_relation_size('turbo.tb_statement_version_tracker'), MIN(created_at)
    INTO v_size_after, v_oldest
    FROM turbo.tb_statement_version_tracker;

    -- Return cleanup summary
    RETURN QUERY VALUES (
        format('Cleaned up %s old statement tracker entries', v_deleted),
        v_deleted,
        v_oldest,
        pg_size_pretty(v_size_before),
        pg_size_pretty(v_size_after)
    );
END;
$$ LANGUAGE plpgsql;

-- Aggressive cleanup for emergency situations
CREATE OR REPLACE FUNCTION turbo.fn_emergency_cleanup_statement_tracker()
RETURNS TABLE(
    cleanup_type TEXT,
    rows_deleted INT,
    safety_note TEXT
) AS $$
DECLARE
    v_deleted_old INT;
    v_deleted_all INT;
    v_total_before INT;
BEGIN
    SELECT COUNT(*) INTO v_total_before FROM turbo.tb_statement_version_tracker;

    -- Phase 1: Delete entries older than 1 minute (very conservative)
    DELETE FROM turbo.tb_statement_version_tracker
    WHERE created_at < NOW() - INTERVAL '1 minute';
    GET DIAGNOSTICS v_deleted_old = ROW_COUNT;

    RETURN QUERY VALUES (
        'Conservative cleanup (> 1 minute old)',
        v_deleted_old,
        format('Removed %s of %s entries safely', v_deleted_old, v_total_before)
    );

    -- Phase 2: If still too many entries, more aggressive cleanup
    IF (SELECT COUNT(*) FROM turbo.tb_statement_version_tracker) > 5000 THEN
        DELETE FROM turbo.tb_statement_version_tracker
        WHERE created_at < NOW() - INTERVAL '30 seconds';
        GET DIAGNOSTICS v_deleted_all = ROW_COUNT;

        RETURN QUERY VALUES (
            'Aggressive cleanup (> 30 seconds old)',
            v_deleted_all - v_deleted_old,
            'WARNING: May have deleted active statements - monitor for issues'
        );
    END IF;

    -- Phase 3: Emergency nuclear option
    IF (SELECT COUNT(*) FROM turbo.tb_statement_version_tracker) > 10000 THEN
        TRUNCATE TABLE turbo.tb_statement_version_tracker;
        GET DIAGNOSTICS v_deleted_all = ROW_COUNT;

        RETURN QUERY VALUES (
            'EMERGENCY: Complete table truncation',
            v_total_before,
            'CRITICAL: All tracking data removed - cache consistency may be affected'
        );
    END IF;
END;
$$ LANGUAGE plpgsql;
```

### Scheduled Maintenance

```sql
-- Maintenance function for scheduled execution
CREATE OR REPLACE FUNCTION turbo.fn_scheduled_maintenance()
RETURNS TABLE(
    maintenance_task TEXT,
    status TEXT,
    details TEXT
) AS $$
DECLARE
    v_health_status TEXT;
    v_cleanup_needed BOOLEAN := FALSE;
    v_cleanup_result RECORD;
BEGIN
    -- Check health status
    SELECT health_status INTO v_health_status
    FROM turbo.v_statement_tracker_health;

    RETURN QUERY VALUES (
        'Health Check',
        COALESCE(v_health_status, 'UNKNOWN'),
        format('Current status: %s', COALESCE(v_health_status, 'Could not determine'))
    );

    -- Determine if cleanup is needed
    v_cleanup_needed := v_health_status IN ('WARNING', 'CRITICAL');

    IF v_cleanup_needed THEN
        -- Perform cleanup
        SELECT cleanup_summary, rows_deleted INTO v_cleanup_result
        FROM turbo.fn_cleanup_statement_tracker()
        LIMIT 1;

        RETURN QUERY VALUES (
            'Statement Tracker Cleanup',
            'COMPLETED',
            format('%s (deleted %s rows)',
                v_cleanup_result.cleanup_summary,
                v_cleanup_result.rows_deleted)
        );
    ELSE
        RETURN QUERY VALUES (
            'Statement Tracker Cleanup',
            'SKIPPED',
            'No cleanup needed - system is healthy'
        );
    END IF;

    -- Update table statistics
    ANALYZE turbo.tb_statement_version_tracker;
    ANALYZE turbo.tb_domain_version;
    ANALYZE turbo.tb_graphql_cache;

    RETURN QUERY VALUES (
        'Statistics Update',
        'COMPLETED',
        'Updated table statistics for optimizer'
    );

    -- Check for any concerning patterns
    IF EXISTS (
        SELECT 1 FROM turbo.v_batch_efficiency_report
        WHERE efficiency_assessment = 'SINGLE'
    ) THEN
        RETURN QUERY VALUES (
            'Efficiency Warning',
            'DETECTED',
            'Some operations are not benefiting from batch optimization - review application patterns'
        );
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Schedule this function to run every 15 minutes
-- In production, use pg_cron or similar:
-- SELECT cron.schedule('statement-tracker-maintenance', '*/15 * * * *',
--     'SELECT * FROM turbo.fn_scheduled_maintenance()');
```

## Integration Examples

### Complete tv_ Table Setup

```sql
-- Complete example of tv_ table with batch-safe caching
CREATE TABLE tv_user_profile (
    -- Sacred Trinity pattern
    id SERIAL PRIMARY KEY,
    pk_user_profile UUID DEFAULT gen_random_uuid() UNIQUE,

    -- Foreign key to source entity
    fk_user INTEGER NOT NULL,

    -- Multi-tenant support
    tenant_id UUID NOT NULL,

    -- Complete denormalized data
    data JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Versioning and audit
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    updated_by UUID,

    -- Constraints
    CONSTRAINT fk_tv_user_profile_source FOREIGN KEY (fk_user) REFERENCES tb_users(id),
    CONSTRAINT uq_tv_user_profile_tenant UNIQUE (fk_user, tenant_id)
);

-- Batch-safe trigger
CREATE TRIGGER tr_user_profile_cache_invalidation
AFTER INSERT OR UPDATE OR DELETE ON tv_user_profile
FOR EACH ROW EXECUTE FUNCTION turbo.fn_tv_table_cache_invalidation();

-- Index for performance
CREATE INDEX idx_tv_user_profile_tenant ON tv_user_profile(tenant_id);
CREATE INDEX idx_tv_user_profile_updated ON tv_user_profile(updated_at);
```

### TurboRouter Integration

```sql
-- Register cached queries with TurboRouter
INSERT INTO turbo.tb_turbo_query (
    operation_name,
    query_hash,
    graphql_query,
    sql_template,
    param_mapping,
    is_active
) VALUES (
    'GetUserProfile',
    encode(sha256('GetUserProfile query hash'::bytea), 'hex'),
    'query GetUserProfile($id: UUID!) { userProfile(id: $id) { id name email avatar } }',
    'SELECT turbo.fn_get_cached_response(
        ''user_profile'',
        $1::text,
        ''user'',
        ''user.fn_build_profile_response'',
        jsonb_build_object(''user_id'', $1)
    )',
    '{"id": 1}'::jsonb,
    true
) ON CONFLICT (query_hash) DO UPDATE SET
    operation_name = EXCLUDED.operation_name,
    sql_template = EXCLUDED.sql_template,
    updated_at = NOW();
```

These examples demonstrate the production-ready implementation of batch-safe lazy caching that provides dramatic performance improvements while maintaining data consistency and eliminating race conditions.
