-- FraiseQL Audit Logging Migration
-- Phase 14: Production-ready audit logging for GraphQL operations
--
-- This migration creates the audit log table with:
-- - Multi-tenant isolation
-- - JSONB for flexible variable storage
-- - Indexes for common query patterns
-- - Time-series optimization (partition-ready)

-- Create audit log table
CREATE TABLE IF NOT EXISTS fraiseql_audit_logs (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    level TEXT NOT NULL CHECK (level IN ('INFO', 'WARN', 'ERROR')),

    -- User context
    user_id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,

    -- Request details
    operation TEXT NOT NULL CHECK (operation IN ('query', 'mutation')),
    query TEXT NOT NULL,
    variables JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Client info
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL,

    -- Error tracking
    error TEXT,

    -- Performance tracking (optional)
    duration_ms INTEGER,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries
-- Index 1: Tenant + timestamp (most common: get recent logs for tenant)
CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_timestamp
    ON fraiseql_audit_logs(tenant_id, timestamp DESC);

-- Index 2: Tenant + level + timestamp (filter by severity)
CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_level
    ON fraiseql_audit_logs(tenant_id, level, timestamp DESC);

-- Index 3: User + timestamp (audit trail for specific user)
CREATE INDEX IF NOT EXISTS idx_audit_logs_user
    ON fraiseql_audit_logs(user_id, timestamp DESC);

-- Index 4: Global timestamp (admin queries across tenants)
CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp
    ON fraiseql_audit_logs(timestamp DESC);

-- Index 5: Variables JSONB (for searching specific query patterns)
CREATE INDEX IF NOT EXISTS idx_audit_logs_variables
    ON fraiseql_audit_logs USING GIN (variables);

-- Comments for documentation
COMMENT ON TABLE fraiseql_audit_logs IS 'Audit log for all GraphQL operations with full context';
COMMENT ON COLUMN fraiseql_audit_logs.tenant_id IS 'Tenant ID for multi-tenant isolation';
COMMENT ON COLUMN fraiseql_audit_logs.variables IS 'GraphQL query variables stored as JSONB';
COMMENT ON COLUMN fraiseql_audit_logs.duration_ms IS 'Query execution time in milliseconds';

-- Optional: Table partitioning for large datasets
-- Uncomment and customize for production with millions of logs
--
-- Example: Partition by month for time-series data
-- ALTER TABLE fraiseql_audit_logs RENAME TO fraiseql_audit_logs_template;
--
-- CREATE TABLE fraiseql_audit_logs (LIKE fraiseql_audit_logs_template INCLUDING ALL)
-- PARTITION BY RANGE (timestamp);
--
-- CREATE TABLE fraiseql_audit_logs_2026_01 PARTITION OF fraiseql_audit_logs
--     FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
--
-- CREATE TABLE fraiseql_audit_logs_2026_02 PARTITION OF fraiseql_audit_logs
--     FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');
--
-- Note: Requires PostgreSQL 10+ for declarative partitioning
