-- SaaS Starter Database Schema with Row-Level Security
-- Multi-tenant SaaS application with PostgreSQL RLS

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- ORGANIZATIONS (TENANTS)
-- ============================================================================

CREATE TABLE tb_organization (
    pk_organization INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(100) UNIQUE NOT NULL,
    plan VARCHAR(50) NOT NULL DEFAULT 'free',
    subscription_status VARCHAR(50) NOT NULL DEFAULT 'trialing',
    stripe_customer_id VARCHAR(255),
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- USERS
-- ============================================================================

CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_organization INT NOT NULL REFERENCES tb_organization(pk_organization) ON DELETE CASCADE,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    avatar_url VARCHAR(500),
    last_active TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on tb_user
ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;

-- Users can only see members of their organization
CREATE POLICY tb_user_tenant_isolation ON tb_user
    FOR ALL
    TO authenticated_user
    USING (fk_organization = current_setting('app.current_tenant', TRUE)::INT);

-- ============================================================================
-- SUBSCRIPTIONS
-- ============================================================================

CREATE TABLE tb_subscription (
    pk_subscription INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_organization INT NOT NULL REFERENCES tb_organization(pk_organization) ON DELETE CASCADE,
    plan VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    amount DECIMAL(10, 2) NOT NULL,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    interval VARCHAR(20) NOT NULL DEFAULT 'month',
    current_period_start TIMESTAMP NOT NULL,
    current_period_end TIMESTAMP NOT NULL,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT FALSE,
    stripe_subscription_id VARCHAR(255) UNIQUE,
    features JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on tb_subscription
ALTER TABLE tb_subscription ENABLE ROW LEVEL SECURITY;

CREATE POLICY tb_subscription_tenant_isolation ON tb_subscription
    FOR ALL
    TO authenticated_user
    USING (fk_organization = current_setting('app.current_tenant', TRUE)::INT);

-- ============================================================================
-- TEAM INVITATIONS
-- ============================================================================

CREATE TABLE tb_team_invitation (
    pk_team_invitation INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_organization INT NOT NULL REFERENCES tb_organization(pk_organization) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    token VARCHAR(255) UNIQUE NOT NULL,
    fk_invited_by INT NOT NULL REFERENCES tb_user(pk_user),
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(fk_organization, email, status)
);

-- Enable RLS on tb_team_invitation
ALTER TABLE tb_team_invitation ENABLE ROW LEVEL SECURITY;

CREATE POLICY tb_team_invitation_tenant_isolation ON tb_team_invitation
    FOR ALL
    TO authenticated_user
    USING (fk_organization = current_setting('app.current_tenant', TRUE)::INT);

-- ============================================================================
-- USAGE TRACKING
-- ============================================================================

CREATE TABLE tb_usage_metric (
    pk_usage_metric INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_organization INT NOT NULL REFERENCES tb_organization(pk_organization) ON DELETE CASCADE,
    period_start TIMESTAMP NOT NULL,
    period_end TIMESTAMP NOT NULL,
    projects INT NOT NULL DEFAULT 0,
    storage BIGINT NOT NULL DEFAULT 0,
    api_calls INT NOT NULL DEFAULT 0,
    seats INT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(fk_organization, period_start)
);

-- Enable RLS on tb_usage_metric
ALTER TABLE tb_usage_metric ENABLE ROW LEVEL SECURITY;

CREATE POLICY tb_usage_metric_tenant_isolation ON tb_usage_metric
    FOR ALL
    TO authenticated_user
    USING (fk_organization = current_setting('app.current_tenant', TRUE)::INT);

-- ============================================================================
-- ACTIVITY LOG
-- ============================================================================

CREATE TABLE tb_activity_log (
    pk_activity_log INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_organization INT NOT NULL REFERENCES tb_organization(pk_organization) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user),
    action VARCHAR(100) NOT NULL,
    resource VARCHAR(50),
    resource_id UUID,
    details JSONB DEFAULT '{}',
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on tb_activity_log
ALTER TABLE tb_activity_log ENABLE ROW LEVEL SECURITY;

CREATE POLICY tb_activity_log_tenant_isolation ON tb_activity_log
    FOR ALL
    TO authenticated_user
    USING (fk_organization = current_setting('app.current_tenant', TRUE)::INT);

-- ============================================================================
-- PROJECTS (EXAMPLE TENANT-AWARE RESOURCE)
-- ============================================================================

CREATE TABLE tb_project (
    pk_project INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_organization INT NOT NULL REFERENCES tb_organization(pk_organization) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    fk_owner INT NOT NULL REFERENCES tb_user(pk_user),
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on tb_project
ALTER TABLE tb_project ENABLE ROW LEVEL SECURITY;

CREATE POLICY tb_project_tenant_isolation ON tb_project
    FOR ALL
    TO authenticated_user
    USING (fk_organization = current_setting('app.current_tenant', TRUE)::INT);

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================

-- Organizations
CREATE INDEX idx_tb_organization_slug ON tb_organization(slug);
CREATE INDEX idx_tb_organization_plan ON tb_organization(plan);
CREATE INDEX idx_tb_organization_status ON tb_organization(subscription_status);

-- Users
CREATE INDEX idx_tb_user_fk_organization ON tb_user(fk_organization);
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_role ON tb_user(role);
CREATE INDEX idx_tb_user_status ON tb_user(status);

-- Subscriptions
CREATE INDEX idx_tb_subscription_fk_organization ON tb_subscription(fk_organization);
CREATE INDEX idx_tb_subscription_status ON tb_subscription(status);
CREATE INDEX idx_tb_subscription_stripe ON tb_subscription(stripe_subscription_id);

-- Team Invitations
CREATE INDEX idx_tb_team_invitation_fk_organization ON tb_team_invitation(fk_organization);
CREATE INDEX idx_tb_team_invitation_email ON tb_team_invitation(email);
CREATE INDEX idx_tb_team_invitation_token ON tb_team_invitation(token);
CREATE INDEX idx_tb_team_invitation_status ON tb_team_invitation(status);

-- Usage Metrics
CREATE INDEX idx_tb_usage_metric_fk_organization_period ON tb_usage_metric(fk_organization, period_start);

-- Activity Log
CREATE INDEX idx_tb_activity_log_fk_organization ON tb_activity_log(fk_organization);
CREATE INDEX idx_tb_activity_log_fk_user ON tb_activity_log(fk_user);
CREATE INDEX idx_tb_activity_log_created ON tb_activity_log(created_at DESC);
CREATE INDEX idx_tb_activity_log_action ON tb_activity_log(action);

-- Projects
CREATE INDEX idx_tb_project_fk_organization ON tb_project(fk_organization);
CREATE INDEX idx_tb_project_fk_owner ON tb_project(fk_owner);
CREATE INDEX idx_tb_project_status ON tb_project(status);
CREATE INDEX idx_tb_project_created ON tb_project(created_at DESC);

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Function to get current organization's member count
CREATE OR REPLACE FUNCTION get_organization_member_count(org_id UUID)
RETURNS INT AS $$
BEGIN
    RETURN (
        SELECT COUNT(*)::INT
        FROM tb_user
        WHERE fk_organization = (SELECT pk_organization FROM tb_organization WHERE id = org_id) AND status = 'active'
    );
END;
$$ LANGUAGE plpgsql;

-- Function to track usage
CREATE OR REPLACE FUNCTION track_usage(
    org_id UUID,
    usage_type VARCHAR,
    amount INT
) RETURNS VOID AS $$
DECLARE
    period_start TIMESTAMP;
    period_end TIMESTAMP;
    org_pk INT;
BEGIN
    -- Get organization pk
    SELECT pk_organization INTO org_pk FROM tb_organization WHERE id = org_id;

    -- Get current billing period
    period_start := DATE_TRUNC('month', CURRENT_TIMESTAMP);
    period_end := period_start + INTERVAL '1 month';

    -- Upsert usage metrics
    INSERT INTO tb_usage_metric (
        fk_organization,
        period_start,
        period_end,
        projects,
        storage,
        api_calls,
        seats
    )
    VALUES (
        org_pk,
        period_start,
        period_end,
        CASE WHEN usage_type = 'projects' THEN amount ELSE 0 END,
        CASE WHEN usage_type = 'storage' THEN amount ELSE 0 END,
        CASE WHEN usage_type = 'api_calls' THEN amount ELSE 0 END,
        CASE WHEN usage_type = 'seats' THEN amount ELSE 0 END
    )
    ON CONFLICT (fk_organization, period_start)
    DO UPDATE SET
        projects = tb_usage_metric.projects + CASE WHEN usage_type = 'projects' THEN amount ELSE 0 END,
        storage = tb_usage_metric.storage + CASE WHEN usage_type = 'storage' THEN amount ELSE 0 END,
        api_calls = tb_usage_metric.api_calls + CASE WHEN usage_type = 'api_calls' THEN amount ELSE 0 END,
        seats = tb_usage_metric.seats + CASE WHEN usage_type = 'seats' THEN amount ELSE 0 END;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- VIEWS
-- ============================================================================

-- Organization view with computed fields
CREATE VIEW v_organization AS
SELECT
    o.pk_organization,
    o.id,
    jsonb_build_object(
        'id', o.id,
        'name', o.name,
        'slug', o.slug,
        'plan', o.plan,
        'subscription_status', o.subscription_status,
        'member_count', get_organization_member_count(o.id),
        'settings', o.settings,
        'created_at', o.created_at,
        'updated_at', o.updated_at
    ) as data
FROM tb_organization o;

-- Users view (excludes password_hash)
CREATE VIEW v_user AS
SELECT
    pk_user,
    id,
    jsonb_build_object(
        'id', id,
        'fk_organization', fk_organization,
        'email', email,
        'name', name,
        'role', role,
        'status', status,
        'avatar_url', avatar_url,
        'last_active', last_active,
        'created_at', created_at
    ) as data
FROM tb_user;

-- Projects view
CREATE VIEW v_project AS
SELECT
    p.pk_project,
    p.id,
    jsonb_build_object(
        'id', p.id,
        'fk_organization', p.fk_organization,
        'name', p.name,
        'description', p.description,
        'fk_owner', p.fk_owner,
        'status', p.status,
        'settings', p.settings,
        'created_at', p.created_at,
        'updated_at', p.updated_at
    ) as data
FROM tb_project p;

-- ============================================================================
-- SAMPLE DATA (FOR TESTING)
-- ============================================================================

-- Create sample organization
INSERT INTO tb_organization (id, name, slug, plan, subscription_status) VALUES
('11111111-1111-1111-1111-111111111111', 'Acme Corp', 'acme-corp', 'professional', 'active'),
('22222222-2222-2222-2222-222222222222', 'Startup Inc', 'startup-inc', 'free', 'trialing');

-- Create sample users
INSERT INTO tb_user (id, fk_organization, email, name, password_hash, role) VALUES
('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', 1, 'founder@acme.com', 'Jane Founder', '$2b$12$dummy_hash', 'owner'),
('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', 1, 'admin@acme.com', 'John Admin', '$2b$12$dummy_hash', 'admin'),
('cccccccc-cccc-cccc-cccc-cccccccccccc', 2, 'founder@startup.com', 'Bob Founder', '$2b$12$dummy_hash', 'owner');

-- Create sample subscription
INSERT INTO tb_subscription (fk_organization, plan, status, amount, interval, current_period_start, current_period_end) VALUES
(1, 'professional', 'active', 99.00, 'month', NOW(), NOW() + INTERVAL '1 month');

-- Create sample projects
INSERT INTO tb_project (fk_organization, name, description, fk_owner) VALUES
(1, 'Product Launch', 'New product launch project', 1),
(2, 'MVP Development', 'Build the MVP', 3);

-- Initialize usage metrics
INSERT INTO tb_usage_metric (fk_organization, period_start, period_end, projects, api_calls, seats) VALUES
(1, DATE_TRUNC('month', NOW()), DATE_TRUNC('month', NOW()) + INTERVAL '1 month', 1, 1250, 2);
