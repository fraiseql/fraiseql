-- Multi-tenant Users Table (tb_user)
-- Stores user account information with tenant isolation

CREATE TABLE tenant.tb_user (
    -- Primary keys and tenant isolation
    pk_user UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fk_organization UUID NOT NULL REFERENCES management.tb_organization(pk_organization),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)

    -- Business identifier (username within tenant)
    identifier CITEXT NOT NULL,

    -- Flat normalized columns
    email CITEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role user_role NOT NULL DEFAULT 'user',
    is_active BOOLEAN NOT NULL DEFAULT true,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    last_login_at TIMESTAMPTZ,

    -- Profile data as JSONB for flexibility
    profile JSONB DEFAULT jsonb_build_object(
        'display_name', '',
        'first_name', '',
        'last_name', '',
        'bio', '',
        'avatar_url', '',
        'timezone', 'UTC',
        'language', 'en'
    ),
    preferences JSONB DEFAULT jsonb_build_object(
        'email_notifications', true,
        'theme', 'auto',
        'posts_per_page', 10
    ),
    metadata JSONB DEFAULT '{}',

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Multi-tenant constraints
    CONSTRAINT unique_username_per_tenant UNIQUE (fk_organization, identifier),
    CONSTRAINT unique_email_per_tenant UNIQUE (fk_organization, email),

    -- Basic constraints
    CONSTRAINT username_length CHECK (length(identifier) >= 3 AND length(identifier) <= 30),
    CONSTRAINT email_format CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
    CONSTRAINT role_tenant_constraint CHECK (
        role IN ('org_admin', 'editor', 'author', 'user', 'guest') OR
        (role = 'platform_admin' AND fk_organization IS NOT NULL)
    )
);

-- Enable Row Level Security
ALTER TABLE tenant.tb_user ENABLE ROW LEVEL SECURITY;

-- RLS Policy: Users can only see users from their own organization
CREATE POLICY user_tenant_isolation ON tenant.tb_user
    FOR ALL
    TO public
    USING (fk_organization = current_tenant_id());

-- Core indexes
CREATE INDEX idx_tb_user_fk_organization ON tenant.tb_user(fk_organization);
CREATE INDEX idx_tb_user_identifier ON tenant.tb_user(fk_organization, identifier);
CREATE INDEX idx_tb_user_pk_user ON tenant.tb_user(pk_user);
CREATE INDEX idx_tb_user_created_at ON tenant.tb_user(created_at DESC);

-- Tenant-aware indexes
CREATE INDEX idx_tb_user_email_tenant ON tenant.tb_user(fk_organization, email);
CREATE INDEX idx_tb_user_role_tenant ON tenant.tb_user(fk_organization, role);
CREATE INDEX idx_tb_user_active_tenant ON tenant.tb_user(fk_organization, is_active);

-- JSONB indexes for profile data
CREATE INDEX idx_tb_user_profile_gin ON tenant.tb_user USING GIN (profile);
CREATE INDEX idx_tb_user_preferences_gin ON tenant.tb_user USING GIN (preferences);

-- Update trigger with tenant validation
CREATE OR REPLACE FUNCTION tenant.update_tb_user_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Validate tenant context
    IF NOT validate_tenant_access(NEW.fk_organization) THEN
        RAISE EXCEPTION 'Access denied: invalid tenant context for user update';
    END IF;

    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE TRIGGER tr_tb_user_updated_at
    BEFORE UPDATE ON tenant.tb_user
    FOR EACH ROW
    EXECUTE FUNCTION tenant.update_tb_user_updated_at();

-- Comments
COMMENT ON TABLE tenant.tb_user IS 'Multi-tenant user accounts with organization isolation';
COMMENT ON COLUMN tenant.tb_user.fk_organization IS 'References the organization this user belongs to';
COMMENT ON COLUMN tenant.tb_user.identifier IS 'Username unique within the organization';
COMMENT ON CONSTRAINT unique_username_per_tenant ON tenant.tb_user IS 'Username must be unique within each organization';
COMMENT ON CONSTRAINT unique_email_per_tenant ON tenant.tb_user IS 'Email must be unique within each organization';
