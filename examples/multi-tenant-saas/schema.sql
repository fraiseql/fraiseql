-- Multi-Tenant SaaS Application Schema
-- Features:
-- - Row-Level Security (RLS) for tenant isolation
-- - Trinity pattern (tb_/v_/tv_) for tables/views/computed views
-- - REGULATED security profile compatibility
-- - Audit trail support

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- BASE TABLES (tb_*)
-- ============================================================================

-- Organizations (Tenants)
CREATE TABLE tb_organization (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    plan TEXT NOT NULL DEFAULT 'free', -- free, starter, professional, enterprise
    status TEXT NOT NULL DEFAULT 'active', -- active, suspended, cancelled
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Users
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES tb_organization(id) ON DELETE CASCADE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'member', -- owner, admin, member, readonly
    status TEXT NOT NULL DEFAULT 'active', -- active, invited, suspended
    last_active_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Projects (tenant-isolated resource)
CREATE TABLE tb_project (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES tb_organization(id) ON DELETE CASCADE,
    owner_id UUID NOT NULL REFERENCES tb_user(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active', -- active, archived, deleted
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Tasks (child resource within project)
CREATE TABLE tb_task (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES tb_organization(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES tb_project(id) ON DELETE CASCADE,
    assigned_to UUID REFERENCES tb_user(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'todo', -- todo, in_progress, done, cancelled
    priority TEXT NOT NULL DEFAULT 'medium', -- low, medium, high, urgent
    due_date TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Audit log (compliance requirement)
CREATE TABLE tb_audit_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES tb_organization(id) ON DELETE CASCADE,
    user_id UUID REFERENCES tb_user(id) ON DELETE SET NULL,
    action TEXT NOT NULL, -- created, updated, deleted, accessed
    resource_type TEXT NOT NULL, -- project, task, user, etc.
    resource_id UUID,
    changes JSONB, -- old/new values for audit trail
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- API usage tracking (for rate limiting/billing)
CREATE TABLE tb_api_usage (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES tb_organization(id) ON DELETE CASCADE,
    user_id UUID REFERENCES tb_user(id) ON DELETE SET NULL,
    endpoint TEXT NOT NULL,
    query_complexity INT DEFAULT 0,
    response_time_ms INT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================

-- Organization lookups
CREATE INDEX idx_organization_slug ON tb_organization(slug);
CREATE INDEX idx_organization_status ON tb_organization(status);

-- User lookups
CREATE INDEX idx_user_organization ON tb_user(organization_id);
CREATE INDEX idx_user_email ON tb_user(email);
CREATE INDEX idx_user_role ON tb_user(organization_id, role);

-- Project lookups (tenant-aware)
CREATE INDEX idx_project_organization ON tb_project(organization_id);
CREATE INDEX idx_project_owner ON tb_project(owner_id);
CREATE INDEX idx_project_org_created ON tb_project(organization_id, created_at DESC);

-- Task lookups (tenant-aware)
CREATE INDEX idx_task_organization ON tb_task(organization_id);
CREATE INDEX idx_task_project ON tb_task(project_id);
CREATE INDEX idx_task_assigned ON tb_task(assigned_to);
CREATE INDEX idx_task_status ON tb_task(organization_id, status);
CREATE INDEX idx_task_due_date ON tb_task(organization_id, due_date) WHERE due_date IS NOT NULL;

-- Audit log lookups
CREATE INDEX idx_audit_organization ON tb_audit_log(organization_id);
CREATE INDEX idx_audit_resource ON tb_audit_log(resource_type, resource_id);
CREATE INDEX idx_audit_created ON tb_audit_log(organization_id, created_at DESC);

-- API usage tracking
CREATE INDEX idx_api_usage_org_created ON tb_api_usage(organization_id, created_at DESC);

-- ============================================================================
-- ROW-LEVEL SECURITY (RLS) POLICIES
-- ============================================================================

-- Enable RLS on all tenant-scoped tables
ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_project ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_task ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_api_usage ENABLE ROW LEVEL SECURITY;

-- Users: Can only see users in their organization
CREATE POLICY user_tenant_isolation ON tb_user
    FOR ALL
    USING (organization_id = current_setting('app.current_tenant_id', true)::UUID);

-- Projects: Can only see projects in their organization
CREATE POLICY project_tenant_isolation ON tb_project
    FOR ALL
    USING (organization_id = current_setting('app.current_tenant_id', true)::UUID);

-- Tasks: Can only see tasks in their organization
CREATE POLICY task_tenant_isolation ON tb_task
    FOR ALL
    USING (organization_id = current_setting('app.current_tenant_id', true)::UUID);

-- Audit logs: Can only see audit logs for their organization
CREATE POLICY audit_log_tenant_isolation ON tb_audit_log
    FOR ALL
    USING (organization_id = current_setting('app.current_tenant_id', true)::UUID);

-- API usage: Can only see API usage for their organization
CREATE POLICY api_usage_tenant_isolation ON tb_api_usage
    FOR ALL
    USING (organization_id = current_setting('app.current_tenant_id', true)::UUID);

-- ============================================================================
-- VIEWS (v_*) - GraphQL-exposed data
-- ============================================================================

-- Organizations view
CREATE VIEW v_organization AS
SELECT
    id,
    name,
    slug,
    plan,
    status,
    settings,
    created_at,
    updated_at
FROM tb_organization;

-- Users view (excluding password hash)
CREATE VIEW v_user AS
SELECT
    id,
    organization_id,
    email,
    name,
    role,
    status,
    last_active_at,
    created_at,
    updated_at
FROM tb_user;

-- Projects view
CREATE VIEW v_project AS
SELECT
    id,
    organization_id,
    owner_id,
    name,
    description,
    status,
    settings,
    created_at,
    updated_at
FROM tb_project;

-- Tasks view
CREATE VIEW v_task AS
SELECT
    id,
    organization_id,
    project_id,
    assigned_to,
    title,
    description,
    status,
    priority,
    due_date,
    completed_at,
    created_at,
    updated_at
FROM tb_task;

-- Audit logs view
CREATE VIEW v_audit_log AS
SELECT
    id,
    organization_id,
    user_id,
    action,
    resource_type,
    resource_id,
    changes,
    ip_address,
    user_agent,
    created_at
FROM tb_audit_log;

-- ============================================================================
-- COMPUTED VIEWS (tv_*) - Denormalized for performance
-- ============================================================================

-- Computed project view with owner details
CREATE VIEW tv_project AS
SELECT
    p.id,
    p.organization_id,
    p.name,
    p.description,
    p.status,
    p.settings,
    p.created_at,
    p.updated_at,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'role', u.role
    ) as owner
FROM tb_project p
JOIN tb_user u ON p.owner_id = u.id;

-- Computed task view with assigned user and project details
CREATE VIEW tv_task AS
SELECT
    t.id,
    t.organization_id,
    t.title,
    t.description,
    t.status,
    t.priority,
    t.due_date,
    t.completed_at,
    t.created_at,
    t.updated_at,
    jsonb_build_object(
        'id', p.id,
        'name', p.name
    ) as project,
    CASE
        WHEN t.assigned_to IS NOT NULL THEN
            jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'email', u.email
            )
        ELSE NULL
    END as assigned_user
FROM tb_task t
JOIN tb_project p ON t.project_id = p.id
LEFT JOIN tb_user u ON t.assigned_to = u.id;

-- Computed organization view with statistics
CREATE VIEW tv_organization AS
SELECT
    o.id,
    o.name,
    o.slug,
    o.plan,
    o.status,
    o.created_at,
    (SELECT COUNT(*) FROM tb_user WHERE organization_id = o.id AND status = 'active') as active_users,
    (SELECT COUNT(*) FROM tb_project WHERE organization_id = o.id AND status = 'active') as active_projects,
    (SELECT COUNT(*) FROM tb_task WHERE organization_id = o.id) as total_tasks,
    (SELECT COUNT(*) FROM tb_task WHERE organization_id = o.id AND status = 'done') as completed_tasks,
    (SELECT COUNT(*) FROM tb_api_usage WHERE organization_id = o.id AND created_at > NOW() - INTERVAL '1 day') as api_calls_today
FROM tb_organization o;

-- ============================================================================
-- FUNCTIONS (fn_*) - Mutations
-- ============================================================================

-- Create new organization (tenant)
CREATE OR REPLACE FUNCTION fn_create_organization(
    input_name TEXT,
    input_slug TEXT,
    input_owner_email TEXT,
    input_owner_password TEXT,
    input_owner_name TEXT
) RETURNS UUID AS $$
DECLARE
    new_org_id UUID;
    new_user_id UUID;
BEGIN
    -- Create organization
    INSERT INTO tb_organization (name, slug, plan, status)
    VALUES (input_name, input_slug, 'free', 'active')
    RETURNING id INTO new_org_id;

    -- Create owner user
    INSERT INTO tb_user (
        organization_id,
        email,
        password_hash,
        name,
        role,
        status
    ) VALUES (
        new_org_id,
        input_owner_email,
        crypt(input_owner_password, gen_salt('bf')),
        input_owner_name,
        'owner',
        'active'
    ) RETURNING id INTO new_user_id;

    -- Log audit event
    INSERT INTO tb_audit_log (
        organization_id,
        user_id,
        action,
        resource_type,
        resource_id
    ) VALUES (
        new_org_id,
        new_user_id,
        'created',
        'organization',
        new_org_id
    );

    RETURN new_org_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Create project
CREATE OR REPLACE FUNCTION fn_create_project(
    input_organization_id UUID,
    input_owner_id UUID,
    input_name TEXT,
    input_description TEXT
) RETURNS UUID AS $$
DECLARE
    new_project_id UUID;
BEGIN
    -- Create project
    INSERT INTO tb_project (
        organization_id,
        owner_id,
        name,
        description,
        status
    ) VALUES (
        input_organization_id,
        input_owner_id,
        input_name,
        input_description,
        'active'
    ) RETURNING id INTO new_project_id;

    -- Log audit event
    INSERT INTO tb_audit_log (
        organization_id,
        user_id,
        action,
        resource_type,
        resource_id
    ) VALUES (
        input_organization_id,
        input_owner_id,
        'created',
        'project',
        new_project_id
    );

    RETURN new_project_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Create task
CREATE OR REPLACE FUNCTION fn_create_task(
    input_organization_id UUID,
    input_project_id UUID,
    input_title TEXT,
    input_description TEXT,
    input_assigned_to UUID,
    input_priority TEXT,
    input_due_date TIMESTAMPTZ
) RETURNS UUID AS $$
DECLARE
    new_task_id UUID;
BEGIN
    -- Create task
    INSERT INTO tb_task (
        organization_id,
        project_id,
        title,
        description,
        assigned_to,
        priority,
        due_date,
        status
    ) VALUES (
        input_organization_id,
        input_project_id,
        input_title,
        input_description,
        input_assigned_to,
        input_priority,
        input_due_date,
        'todo'
    ) RETURNING id INTO new_task_id;

    -- Log audit event
    INSERT INTO tb_audit_log (
        organization_id,
        action,
        resource_type,
        resource_id
    ) VALUES (
        input_organization_id,
        'created',
        'task',
        new_task_id
    );

    RETURN new_task_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Update task status
CREATE OR REPLACE FUNCTION fn_update_task_status(
    input_task_id UUID,
    input_status TEXT
) RETURNS UUID AS $$
DECLARE
    task_org_id UUID;
BEGIN
    -- Get organization ID for audit log
    SELECT organization_id INTO task_org_id
    FROM tb_task
    WHERE id = input_task_id;

    -- Update task
    UPDATE tb_task
    SET
        status = input_status,
        completed_at = CASE WHEN input_status = 'done' THEN NOW() ELSE NULL END,
        updated_at = NOW()
    WHERE id = input_task_id;

    -- Log audit event
    INSERT INTO tb_audit_log (
        organization_id,
        action,
        resource_type,
        resource_id,
        changes
    ) VALUES (
        task_org_id,
        'updated',
        'task',
        input_task_id,
        jsonb_build_object('status', input_status)
    );

    RETURN input_task_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Invite user to organization
CREATE OR REPLACE FUNCTION fn_invite_user(
    input_organization_id UUID,
    input_email TEXT,
    input_name TEXT,
    input_role TEXT
) RETURNS UUID AS $$
DECLARE
    new_user_id UUID;
    temp_password TEXT;
BEGIN
    -- Generate temporary password
    temp_password := gen_random_uuid()::TEXT;

    -- Create invited user
    INSERT INTO tb_user (
        organization_id,
        email,
        password_hash,
        name,
        role,
        status
    ) VALUES (
        input_organization_id,
        input_email,
        crypt(temp_password, gen_salt('bf')),
        input_name,
        input_role,
        'invited'
    ) RETURNING id INTO new_user_id;

    -- Log audit event
    INSERT INTO tb_audit_log (
        organization_id,
        action,
        resource_type,
        resource_id
    ) VALUES (
        input_organization_id,
        'invited',
        'user',
        new_user_id
    );

    RETURN new_user_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- ============================================================================
-- SEED DATA (for testing)
-- ============================================================================

-- Sample organization
INSERT INTO tb_organization (id, name, slug, plan, status)
VALUES
    ('11111111-1111-1111-1111-111111111111', 'Acme Corporation', 'acme', 'professional', 'active'),
    ('22222222-2222-2222-2222-222222222222', 'Beta Industries', 'beta', 'starter', 'active');

-- Sample users
INSERT INTO tb_user (id, organization_id, email, password_hash, name, role, status)
VALUES
    -- Acme users
    ('11111111-1111-1111-1111-111111111112', '11111111-1111-1111-1111-111111111111', 'alice@acme.com', crypt('password123', gen_salt('bf')), 'Alice Admin', 'owner', 'active'),
    ('11111111-1111-1111-1111-111111111113', '11111111-1111-1111-1111-111111111111', 'bob@acme.com', crypt('password123', gen_salt('bf')), 'Bob Builder', 'member', 'active'),
    ('11111111-1111-1111-1111-111111111114', '11111111-1111-1111-1111-111111111111', 'carol@acme.com', crypt('password123', gen_salt('bf')), 'Carol Contributor', 'member', 'active'),
    -- Beta users
    ('22222222-2222-2222-2222-222222222223', '22222222-2222-2222-2222-222222222222', 'dave@beta.com', crypt('password123', gen_salt('bf')), 'Dave Director', 'owner', 'active'),
    ('22222222-2222-2222-2222-222222222224', '22222222-2222-2222-2222-222222222222', 'eve@beta.com', crypt('password123', gen_salt('bf')), 'Eve Engineer', 'member', 'active');

-- Sample projects
INSERT INTO tb_project (id, organization_id, owner_id, name, description, status)
VALUES
    -- Acme projects
    ('11111111-1111-1111-1111-111111111115', '11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111112', 'Website Redesign', 'Redesign company website', 'active'),
    ('11111111-1111-1111-1111-111111111116', '11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111113', 'Mobile App', 'Build iOS and Android apps', 'active'),
    -- Beta projects
    ('22222222-2222-2222-2222-222222222225', '22222222-2222-2222-2222-222222222222', '22222222-2222-2222-2222-222222222223', 'Product Launch', 'Q1 2024 product launch', 'active');

-- Sample tasks
INSERT INTO tb_task (organization_id, project_id, assigned_to, title, description, status, priority, due_date)
VALUES
    -- Acme tasks
    ('11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111115', '11111111-1111-1111-1111-111111111113', 'Design homepage mockup', 'Create Figma mockup for new homepage', 'in_progress', 'high', NOW() + INTERVAL '3 days'),
    ('11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111115', '11111111-1111-1111-1111-111111111114', 'Write homepage copy', 'Draft marketing copy for homepage', 'todo', 'medium', NOW() + INTERVAL '5 days'),
    ('11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111116', '11111111-1111-1111-1111-111111111113', 'Setup React Native project', 'Initialize RN project with TypeScript', 'done', 'high', NOW() - INTERVAL '2 days'),
    -- Beta tasks
    ('22222222-2222-2222-2222-222222222222', '22222222-2222-2222-2222-222222222225', '22222222-2222-2222-2222-222222222224', 'Finalize product specs', 'Complete technical specifications', 'in_progress', 'urgent', NOW() + INTERVAL '1 day');

-- Sample audit logs
INSERT INTO tb_audit_log (organization_id, user_id, action, resource_type, resource_id)
VALUES
    ('11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111112', 'created', 'project', '11111111-1111-1111-1111-111111111115'),
    ('11111111-1111-1111-1111-111111111111', '11111111-1111-1111-1111-111111111113', 'created', 'task', NULL),
    ('22222222-2222-2222-2222-222222222222', '22222222-2222-2222-2222-222222222223', 'created', 'project', '22222222-2222-2222-2222-222222222225');

-- ============================================================================
-- COMMENTS FOR DOCUMENTATION
-- ============================================================================

COMMENT ON TABLE tb_organization IS 'Tenant/organization table - each row is a separate customer';
COMMENT ON TABLE tb_user IS 'Users belong to organizations - isolated by RLS';
COMMENT ON TABLE tb_project IS 'Projects belong to organizations - isolated by RLS';
COMMENT ON TABLE tb_task IS 'Tasks belong to projects and organizations - isolated by RLS';
COMMENT ON TABLE tb_audit_log IS 'Compliance audit trail - all actions logged';
COMMENT ON TABLE tb_api_usage IS 'API usage tracking for rate limiting and billing';

COMMENT ON POLICY user_tenant_isolation ON tb_user IS 'Users can only see other users in their organization';
COMMENT ON POLICY project_tenant_isolation ON tb_project IS 'Projects are isolated by organization_id';
COMMENT ON POLICY task_tenant_isolation ON tb_task IS 'Tasks are isolated by organization_id';
