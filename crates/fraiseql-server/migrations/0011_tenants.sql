-- Phase 11.6 Cycle 2: Multi-Tenancy Schema
-- Tenant management and isolation

CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    slug VARCHAR(255) UNIQUE,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'::jsonb,
    is_active BOOLEAN DEFAULT true
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_tenants_name ON tenants(name);
CREATE INDEX IF NOT EXISTS idx_tenants_slug ON tenants(slug);
CREATE INDEX IF NOT EXISTS idx_tenants_is_active ON tenants(is_active);

-- Add tenant isolation to existing tables (if they exist)
-- These statements will succeed or be silently ignored if columns don't exist

-- Add tenant_id to users table if users table exists and doesn't have tenant_id
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'users') THEN
        IF NOT EXISTS (
            SELECT FROM information_schema.columns
            WHERE table_name = 'users' AND column_name = 'tenant_id'
        ) THEN
            ALTER TABLE users ADD COLUMN tenant_id UUID;
            ALTER TABLE users ADD CONSTRAINT fk_users_tenant_id FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
            CREATE INDEX idx_users_tenant_id ON users(tenant_id);
        END IF;
    END IF;
END
$$;

-- Add tenant_id to audit_log if not present
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT FROM information_schema.columns
        WHERE table_name = 'audit_log' AND column_name = 'tenant_id'
    ) THEN
        ALTER TABLE audit_log ADD COLUMN tenant_id UUID;
        ALTER TABLE audit_log ADD CONSTRAINT fk_audit_log_tenant_id FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
        CREATE INDEX idx_audit_log_tenant_id ON audit_log(tenant_id);
    END IF;
END
$$;
