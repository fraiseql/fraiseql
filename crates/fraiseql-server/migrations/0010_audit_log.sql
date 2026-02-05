-- Phase 11.6 Cycle 1: Audit Log Schema
-- Audit logging table with comprehensive event tracking

CREATE TABLE IF NOT EXISTS audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255),
    username VARCHAR(255),
    ip_address INET,
    resource_type VARCHAR(255),
    resource_id VARCHAR(255),
    action VARCHAR(255),
    before_state JSONB,
    after_state JSONB,
    status VARCHAR(50) NOT NULL DEFAULT 'success',  -- success, failure, denied
    error_message TEXT,
    tenant_id UUID,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_user_id ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_event_type ON audit_log(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_log_status ON audit_log(status);
CREATE INDEX IF NOT EXISTS idx_audit_log_tenant_id ON audit_log(tenant_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_composite ON audit_log(tenant_id, timestamp DESC);

-- Composite index for common filter patterns
CREATE INDEX IF NOT EXISTS idx_audit_log_event_time ON audit_log(event_type, timestamp DESC);
