-- Native Authentication Schema for FraiseQL
-- This migration creates all tables required for native JWT authentication

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- User table for authentication
CREATE TABLE IF NOT EXISTS tb_user (
    pk_user UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    roles TEXT[] DEFAULT '{}',
    permissions TEXT[] DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    is_active BOOLEAN DEFAULT true,
    email_verified BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uk_user_email UNIQUE (email)
);

-- Index for email lookups
CREATE INDEX IF NOT EXISTS idx_user_email ON tb_user(email);
CREATE INDEX IF NOT EXISTS idx_user_active ON tb_user(is_active) WHERE is_active = true;

-- Session tracking table
CREATE TABLE IF NOT EXISTS tb_session (
    pk_session UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fk_user UUID NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    token_family UUID NOT NULL,
    user_agent TEXT,
    ip_address INET,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    last_active TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT fk_session_user FOREIGN KEY (fk_user) REFERENCES tb_user(pk_user) ON DELETE CASCADE
);

-- Index for fast family lookups (only active sessions)
CREATE INDEX IF NOT EXISTS idx_session_family ON tb_session(token_family) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_session_user ON tb_session(fk_user) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_session_last_active ON tb_session(last_active);

-- Used refresh tokens to prevent replay attacks
CREATE TABLE IF NOT EXISTS tb_used_refresh_token (
    token_jti TEXT PRIMARY KEY,
    family_id UUID NOT NULL,
    used_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Index for cleanup of old tokens
CREATE INDEX IF NOT EXISTS idx_used_token_cleanup ON tb_used_refresh_token(used_at);

-- Password reset tokens
CREATE TABLE IF NOT EXISTS tb_password_reset (
    pk_reset UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fk_user UUID NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    CONSTRAINT fk_reset_user FOREIGN KEY (fk_user) REFERENCES tb_user(pk_user) ON DELETE CASCADE
);

-- Index for token lookups and cleanup
CREATE INDEX IF NOT EXISTS idx_password_reset_token ON tb_password_reset(token_hash) WHERE used_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_password_reset_expires ON tb_password_reset(expires_at);

-- Authentication audit trail
CREATE TABLE IF NOT EXISTS tb_auth_audit (
    pk_audit UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL CHECK (event_type IN (
        'login_success', 'login_failure', 'logout', 'register',
        'password_reset_requested', 'password_reset_completed',
        'token_refresh', 'token_theft_detected', 'session_revoked',
        'account_locked', 'account_unlocked'
    )),
    user_id UUID,
    ip_address INET,
    user_agent TEXT,
    event_data JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for audit queries
CREATE INDEX IF NOT EXISTS idx_audit_user ON tb_auth_audit(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_event ON tb_auth_audit(event_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_created ON tb_auth_audit(created_at DESC);

-- Update timestamp trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply update trigger to user table
CREATE TRIGGER update_tb_user_updated_at BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Create view for FraiseQL compatibility (with JSONB data column)
CREATE OR REPLACE VIEW user_view AS
SELECT 
    pk_user as id,
    jsonb_build_object(
        'id', pk_user,
        'email', email,
        'name', name,
        'roles', roles,
        'permissions', permissions,
        'metadata', metadata,
        'isActive', is_active,
        'emailVerified', email_verified,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) as data
FROM tb_user;

-- Grant permissions (adjust based on your database user)
-- GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO your_app_user;
-- GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO your_app_user;