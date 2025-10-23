-- SaaS Starter Database Schema with Row-Level Security
-- Multi-tenant SaaS application with PostgreSQL RLS

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- ORGANIZATIONS (TENANTS)
-- ============================================================================

CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
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

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    avatar_url VARCHAR(500),
    last_active TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on users
ALTER TABLE users ENABLE ROW LEVEL SECURITY;

-- Users can only see members of their organization
CREATE POLICY users_tenant_isolation ON users
    FOR ALL
    TO authenticated_user
    USING (organization_id = current_setting('app.current_tenant', TRUE)::UUID);

-- ============================================================================
-- SUBSCRIPTIONS
-- ============================================================================

CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
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

-- Enable RLS on subscriptions
ALTER TABLE subscriptions ENABLE ROW LEVEL SECURITY;

CREATE POLICY subscriptions_tenant_isolation ON subscriptions
    FOR ALL
    TO authenticated_user
    USING (organization_id = current_setting('app.current_tenant', TRUE)::UUID);

-- ============================================================================
-- TEAM INVITATIONS
-- ============================================================================

CREATE TABLE team_invitations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    token VARCHAR(255) UNIQUE NOT NULL,
    invited_by_id UUID NOT NULL REFERENCES users(id),
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(organization_id, email, status)
);

-- Enable RLS on team_invitations
ALTER TABLE team_invitations ENABLE ROW LEVEL SECURITY;

CREATE POLICY invitations_tenant_isolation ON team_invitations
    FOR ALL
    TO authenticated_user
    USING (organization_id = current_setting('app.current_tenant', TRUE)::UUID);

-- ============================================================================
-- USAGE TRACKING
-- ============================================================================

CREATE TABLE usage_metrics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    period_start TIMESTAMP NOT NULL,
    period_end TIMESTAMP NOT NULL,
    projects INT NOT NULL DEFAULT 0,
    storage BIGINT NOT NULL DEFAULT 0,
    api_calls INT NOT NULL DEFAULT 0,
    seats INT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(organization_id, period_start)
);

-- Enable RLS on usage_metrics
ALTER TABLE usage_metrics ENABLE ROW LEVEL SECURITY;

CREATE POLICY usage_metrics_tenant_isolation ON usage_metrics
    FOR ALL
    TO authenticated_user
    USING (organization_id = current_setting('app.current_tenant', TRUE)::UUID);

-- ============================================================================
-- ACTIVITY LOG
-- ============================================================================

CREATE TABLE activity_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    action VARCHAR(100) NOT NULL,
    resource VARCHAR(50),
    resource_id UUID,
    details JSONB DEFAULT '{}',
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on activity_log
ALTER TABLE activity_log ENABLE ROW LEVEL SECURITY;

CREATE POLICY activity_log_tenant_isolation ON activity_log
    FOR ALL
    TO authenticated_user
    USING (organization_id = current_setting('app.current_tenant', TRUE)::UUID);

-- ============================================================================
-- PROJECTS (EXAMPLE TENANT-AWARE RESOURCE)
-- ============================================================================

CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES users(id),
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Enable RLS on projects
ALTER TABLE projects ENABLE ROW LEVEL SECURITY;

CREATE POLICY projects_tenant_isolation ON projects
    FOR ALL
    TO authenticated_user
    USING (organization_id = current_setting('app.current_tenant', TRUE)::UUID);

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================

-- Organizations
CREATE INDEX idx_organizations_slug ON organizations(slug);
CREATE INDEX idx_organizations_plan ON organizations(plan);
CREATE INDEX idx_organizations_status ON organizations(subscription_status);

-- Users
CREATE INDEX idx_users_organization ON users(organization_id);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_users_status ON users(status);

-- Subscriptions
CREATE INDEX idx_subscriptions_organization ON subscriptions(organization_id);
CREATE INDEX idx_subscriptions_status ON subscriptions(status);
CREATE INDEX idx_subscriptions_stripe ON subscriptions(stripe_subscription_id);

-- Team Invitations
CREATE INDEX idx_invitations_organization ON team_invitations(organization_id);
CREATE INDEX idx_invitations_email ON team_invitations(email);
CREATE INDEX idx_invitations_token ON team_invitations(token);
CREATE INDEX idx_invitations_status ON team_invitations(status);

-- Usage Metrics
CREATE INDEX idx_usage_organization_period ON usage_metrics(organization_id, period_start);

-- Activity Log
CREATE INDEX idx_activity_organization ON activity_log(organization_id);
CREATE INDEX idx_activity_user ON activity_log(user_id);
CREATE INDEX idx_activity_created ON activity_log(created_at DESC);
CREATE INDEX idx_activity_action ON activity_log(action);

-- Projects
CREATE INDEX idx_projects_organization ON projects(organization_id);
CREATE INDEX idx_projects_owner ON projects(owner_id);
CREATE INDEX idx_projects_status ON projects(status);
CREATE INDEX idx_projects_created ON projects(created_at DESC);

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Function to get current organization's member count
CREATE OR REPLACE FUNCTION get_organization_member_count(org_id UUID)
RETURNS INT AS $$
BEGIN
    RETURN (
        SELECT COUNT(*)::INT
        FROM users
        WHERE organization_id = org_id AND status = 'active'
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
BEGIN
    -- Get current billing period
    period_start := DATE_TRUNC('month', CURRENT_TIMESTAMP);
    period_end := period_start + INTERVAL '1 month';

    -- Upsert usage metrics
    INSERT INTO usage_metrics (
        organization_id,
        period_start,
        period_end,
        projects,
        storage,
        api_calls,
        seats
    )
    VALUES (
        org_id,
        period_start,
        period_end,
        CASE WHEN usage_type = 'projects' THEN amount ELSE 0 END,
        CASE WHEN usage_type = 'storage' THEN amount ELSE 0 END,
        CASE WHEN usage_type = 'api_calls' THEN amount ELSE 0 END,
        CASE WHEN usage_type = 'seats' THEN amount ELSE 0 END
    )
    ON CONFLICT (organization_id, period_start)
    DO UPDATE SET
        projects = usage_metrics.projects + CASE WHEN usage_type = 'projects' THEN amount ELSE 0 END,
        storage = usage_metrics.storage + CASE WHEN usage_type = 'storage' THEN amount ELSE 0 END,
        api_calls = usage_metrics.api_calls + CASE WHEN usage_type = 'api_calls' THEN amount ELSE 0 END,
        seats = usage_metrics.seats + CASE WHEN usage_type = 'seats' THEN amount ELSE 0 END;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- VIEWS
-- ============================================================================

-- Organization view with computed fields
CREATE VIEW organizations_view AS
SELECT
    o.id,
    o.name,
    o.slug,
    o.plan,
    o.subscription_status,
    get_organization_member_count(o.id) as member_count,
    o.settings,
    o.created_at,
    o.updated_at
FROM organizations o;

-- Users view (excludes password_hash)
CREATE VIEW users_view AS
SELECT
    id,
    organization_id,
    email,
    name,
    role,
    status,
    avatar_url,
    last_active,
    created_at
FROM users;

-- Projects view
CREATE VIEW projects_view AS
SELECT
    p.id,
    p.organization_id,
    p.name,
    p.description,
    p.owner_id,
    p.status,
    p.settings,
    p.created_at,
    p.updated_at
FROM projects p;

-- ============================================================================
-- SAMPLE DATA (FOR TESTING)
-- ============================================================================

-- Create sample organization
INSERT INTO organizations (id, name, slug, plan, subscription_status) VALUES
('11111111-1111-1111-1111-111111111111', 'Acme Corp', 'acme-corp', 'professional', 'active'),
('22222222-2222-2222-2222-222222222222', 'Startup Inc', 'startup-inc', 'free', 'trialing');

-- Create sample users
INSERT INTO users (id, organization_id, email, name, password_hash, role) VALUES
('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111', 'founder@acme.com', 'Jane Founder', '$2b$12$dummy_hash', 'owner'),
('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', '11111111-1111-1111-1111-111111111111', 'admin@acme.com', 'John Admin', '$2b$12$dummy_hash', 'admin'),
('cccccccc-cccc-cccc-cccc-cccccccccccc', '22222222-2222-2222-2222-222222222222', 'founder@startup.com', 'Bob Founder', '$2b$12$dummy_hash', 'owner');

-- Create sample subscription
INSERT INTO subscriptions (organization_id, plan, status, amount, interval, current_period_start, current_period_end) VALUES
('11111111-1111-1111-1111-111111111111', 'professional', 'active', 99.00, 'month', NOW(), NOW() + INTERVAL '1 month');

-- Create sample projects
INSERT INTO projects (organization_id, name, description, owner_id) VALUES
('11111111-1111-1111-1111-111111111111', 'Product Launch', 'New product launch project', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'),
('22222222-2222-2222-2222-222222222222', 'MVP Development', 'Build the MVP', 'cccccccc-cccc-cccc-cccc-cccccccccccc');

-- Initialize usage metrics
INSERT INTO usage_metrics (organization_id, period_start, period_end, projects, api_calls, seats) VALUES
('11111111-1111-1111-1111-111111111111', DATE_TRUNC('month', NOW()), DATE_TRUNC('month', NOW()) + INTERVAL '1 month', 1, 1250, 2);
