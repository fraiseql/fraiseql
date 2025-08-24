-- Blog Demo Enterprise Organizations Table
-- Multi-tenant blog hosting organizations (tenants)

CREATE TABLE management.tb_organization (
    -- Primary key
    pk_organization UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    
    -- Organization identification
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    identifier TEXT NOT NULL UNIQUE CHECK (
        identifier ~ '^[a-z0-9][a-z0-9-]*[a-z0-9]$' AND 
        length(identifier) BETWEEN 2 AND 50
    ),
    
    -- Contact information
    contact_email CITEXT NOT NULL CHECK (contact_email ~ '^[^@]+@[^@]+\.[^@]+$'),
    website_url TEXT CHECK (website_url IS NULL OR website_url ~ '^https?://'),
    
    -- Subscription and billing
    subscription_plan subscription_plan NOT NULL DEFAULT 'starter',
    status organization_status NOT NULL DEFAULT 'trial',
    
    -- Limits and quotas (stored as JSONB for flexibility)
    limits JSONB NOT NULL DEFAULT jsonb_build_object(
        'max_users', 5,
        'max_posts_per_month', 50,
        'max_storage_mb', 100,
        'max_api_requests_per_day', 1000
    ),
    
    -- Settings (theme, branding, etc.)
    settings JSONB NOT NULL DEFAULT jsonb_build_object(
        'theme', 'default',
        'allow_user_registration', true,
        'moderation_required', false,
        'custom_domain', null
    ),
    
    -- Audit fields
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID, -- Initially null for platform setup
    updated_by UUID
);

-- Indexes for performance
CREATE INDEX idx_organization_identifier ON management.tb_organization (identifier);
CREATE INDEX idx_organization_status ON management.tb_organization (status);
CREATE INDEX idx_organization_subscription_plan ON management.tb_organization (subscription_plan);
CREATE INDEX idx_organization_created_at ON management.tb_organization (created_at DESC);

-- Update timestamp trigger
CREATE OR REPLACE FUNCTION management.update_organization_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_organization_updated_at
    BEFORE UPDATE ON management.tb_organization
    FOR EACH ROW
    EXECUTE FUNCTION management.update_organization_timestamp();

-- Comments for documentation
COMMENT ON TABLE management.tb_organization IS 'Multi-tenant blog hosting organizations';
COMMENT ON COLUMN management.tb_organization.pk_organization IS 'Unique organization identifier used as tenant_id';
COMMENT ON COLUMN management.tb_organization.identifier IS 'URL-safe organization identifier (subdomain)';
COMMENT ON COLUMN management.tb_organization.limits IS 'JSON object with usage limits and quotas';
COMMENT ON COLUMN management.tb_organization.settings IS 'JSON object with organization settings and preferences';