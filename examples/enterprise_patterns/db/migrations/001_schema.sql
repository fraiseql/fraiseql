-- Enterprise Patterns Schema
-- Complete schema demonstrating all FraiseQL enterprise patterns

-- Extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Schemas
CREATE SCHEMA IF NOT EXISTS tenant;
CREATE SCHEMA IF NOT EXISTS app;
CREATE SCHEMA IF NOT EXISTS core;

-- Unified audit table with cryptographic chain integrity
-- Combines CDC (old_data, new_data) + tamper-proof chain (hash, signature)
CREATE TABLE audit_events (
    -- Primary identification
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Multi-tenancy
    tenant_id UUID NOT NULL,
    user_id UUID,

    -- Entity tracking (CDC)
    entity_type TEXT NOT NULL,
    entity_id UUID,

    -- Operation classification
    operation_type TEXT NOT NULL, -- INSERT, UPDATE, DELETE, NOOP
    operation_subtype TEXT,       -- new, updated, noop:duplicate, etc.

    -- Change tracking (Debezium-style CDC)
    changed_fields TEXT[],
    old_data JSONB,
    new_data JSONB,

    -- Business metadata
    metadata JSONB,               -- business_actions, validation results, etc.

    -- Context
    ip_address INET,
    correlation_id UUID DEFAULT gen_random_uuid(),

    -- Timestamps
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Cryptographic chain integrity
    previous_hash VARCHAR(64),    -- Links to previous event in tenant chain
    event_hash VARCHAR(64) NOT NULL,
    signature VARCHAR(128) NOT NULL
);

-- Enable RLS for tamper-proof audit
ALTER TABLE audit_events ENABLE ROW LEVEL SECURITY;

-- Append-only policy: Allow INSERT, prevent UPDATE/DELETE
CREATE POLICY audit_events_insert_only ON audit_events
    FOR ALL
    USING (false)          -- No SELECT/UPDATE/DELETE allowed via this policy
    WITH CHECK (true);     -- INSERT allowed

-- Organizations table (multi-tenant root)
CREATE TABLE tenant.tb_organization (
    pk_organization INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_by UUID,
    deleted_at TIMESTAMP WITH TIME ZONE,
    version INTEGER NOT NULL DEFAULT 1,

    -- Enterprise features
    identifier TEXT GENERATED ALWAYS AS (data->>'identifier') STORED,
    name TEXT GENERATED ALWAYS AS (data->>'name') STORED,
    is_active BOOLEAN GENERATED ALWAYS AS ((data->>'is_active')::BOOLEAN) STORED
);

-- Users table with full audit trail
CREATE TABLE tenant.tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    pk_organization INT NOT NULL REFERENCES tenant.tb_organization(pk_organization),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_by UUID,
    deleted_at TIMESTAMP WITH TIME ZONE,
    version INTEGER NOT NULL DEFAULT 1,

    -- Enterprise features
    identifier TEXT GENERATED ALWAYS AS (data->>'identifier') STORED,
    email TEXT GENERATED ALWAYS AS (lower(data->>'email')) STORED,
    is_active BOOLEAN GENERATED ALWAYS AS ((data->>'is_active')::BOOLEAN) STORED,
    last_login_at TIMESTAMP WITH TIME ZONE GENERATED ALWAYS AS (
        CASE WHEN data->>'last_login_at' IS NOT NULL
        THEN (data->>'last_login_at')::TIMESTAMP WITH TIME ZONE
        ELSE NULL END
    ) STORED
);

-- Projects table with business logic
CREATE TABLE tenant.tb_project (
    pk_project INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    pk_organization INT NOT NULL REFERENCES tenant.tb_organization(pk_organization),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_by INT REFERENCES tenant.tb_user(pk_user),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_by INT REFERENCES tenant.tb_user(pk_user),
    deleted_at TIMESTAMP WITH TIME ZONE,
    version INTEGER NOT NULL DEFAULT 1,

    -- Enterprise features
    identifier TEXT GENERATED ALWAYS AS (data->>'identifier') STORED,
    name TEXT GENERATED ALWAYS AS (data->>'name') STORED,
    status TEXT GENERATED ALWAYS AS (data->>'status') STORED,
    owner_id UUID GENERATED ALWAYS AS ((data->>'owner_id')::UUID) STORED,
    due_date TIMESTAMP WITH TIME ZONE GENERATED ALWAYS AS (
        CASE WHEN data->>'due_date' IS NOT NULL
        THEN (data->>'due_date')::TIMESTAMP WITH TIME ZONE
        ELSE NULL END
    ) STORED
);

-- Tasks table with complex relationships
CREATE TABLE tenant.tb_task (
    pk_task INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    pk_organization INT NOT NULL REFERENCES tenant.tb_organization(pk_organization),
    pk_project INT NOT NULL REFERENCES tenant.tb_project(pk_project),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_by INT REFERENCES tenant.tb_user(pk_user),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_by INT REFERENCES tenant.tb_user(pk_user),
    deleted_at TIMESTAMP WITH TIME ZONE,
    version INTEGER NOT NULL DEFAULT 1,

    -- Enterprise features
    identifier TEXT GENERATED ALWAYS AS (data->>'identifier') STORED,
    title TEXT GENERATED ALWAYS AS (data->>'title') STORED,
    status TEXT GENERATED ALWAYS AS (data->>'status') STORED,
    assignee_id UUID GENERATED ALWAYS AS ((data->>'assignee_id')::UUID) STORED,
    parent_task_id UUID GENERATED ALWAYS AS ((data->>'parent_task_id')::UUID) STORED,
    due_date TIMESTAMP WITH TIME ZONE GENERATED ALWAYS AS (
        CASE WHEN data->>'due_date' IS NOT NULL
        THEN (data->>'due_date')::TIMESTAMP WITH TIME ZONE
        ELSE NULL END
    ) STORED
);

-- Document versioning table (demonstrating file management patterns)
CREATE TABLE tenant.tb_document (
    pk_document INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    pk_organization INT NOT NULL REFERENCES tenant.tb_organization(pk_organization),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_by INT REFERENCES tenant.tb_user(pk_user),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_by INT REFERENCES tenant.tb_user(pk_user),
    deleted_at TIMESTAMP WITH TIME ZONE,
    version INTEGER NOT NULL DEFAULT 1,

    -- Enterprise features
    identifier TEXT GENERATED ALWAYS AS (data->>'identifier') STORED,
    filename TEXT GENERATED ALWAYS AS (data->>'filename') STORED,
    content_type TEXT GENERATED ALWAYS AS (data->>'content_type') STORED,
    file_size BIGINT GENERATED ALWAYS AS ((data->>'file_size')::BIGINT) STORED
);

-- Notifications table (event-driven patterns)
CREATE TABLE tenant.tb_notification (
    pk_notification INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    pk_organization INT NOT NULL REFERENCES tenant.tb_organization(pk_organization),
    pk_user INT NOT NULL REFERENCES tenant.tb_user(pk_user),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    read_at TIMESTAMP WITH TIME ZONE,

    -- Notification metadata
    notification_type TEXT GENERATED ALWAYS AS (data->>'type') STORED,
    priority TEXT GENERATED ALWAYS AS (data->>'priority') STORED,
    entity_type TEXT GENERATED ALWAYS AS (data->>'entity_type') STORED,
    entity_id UUID GENERATED ALWAYS AS ((data->>'entity_id')::UUID) STORED
);

-- Indexes for performance and enterprise features

-- Unified audit table indexes
CREATE INDEX idx_audit_events_tenant_time ON audit_events(tenant_id, timestamp DESC);
CREATE INDEX idx_audit_events_entity ON audit_events(entity_type, entity_id);
CREATE INDEX idx_audit_events_hash ON audit_events(event_hash);
CREATE INDEX idx_audit_events_correlation ON audit_events(correlation_id);
CREATE INDEX idx_audit_events_operation ON audit_events(operation_type, operation_subtype);

-- GIN index for metadata queries
CREATE INDEX idx_audit_events_metadata ON audit_events USING GIN (metadata);

-- Organization indexes
CREATE UNIQUE INDEX idx_organization_identifier ON tenant.tb_organization (identifier) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_organization_name ON tenant.tb_organization (name) WHERE deleted_at IS NULL AND is_active;
CREATE INDEX idx_organization_active ON tenant.tb_organization (is_active, created_at) WHERE deleted_at IS NULL;

-- User indexes
CREATE UNIQUE INDEX idx_user_email_org ON tenant.tb_user (pk_organization, email) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_user_identifier ON tenant.tb_user (identifier) WHERE deleted_at IS NULL;
CREATE INDEX idx_user_organization_active ON tenant.tb_user (pk_organization, is_active) WHERE deleted_at IS NULL;
CREATE INDEX idx_user_last_login ON tenant.tb_user (last_login_at DESC) WHERE deleted_at IS NULL;

-- Project indexes
CREATE UNIQUE INDEX idx_project_name_org ON tenant.tb_project (pk_organization, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_project_identifier ON tenant.tb_project (identifier) WHERE deleted_at IS NULL;
CREATE INDEX idx_project_organization_status ON tenant.tb_project (pk_organization, status);
CREATE INDEX idx_project_owner ON tenant.tb_project (owner_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_project_due_date ON tenant.tb_project (due_date) WHERE deleted_at IS NULL AND status IN ('active', 'on_hold');

-- Task indexes
CREATE UNIQUE INDEX idx_task_identifier ON tenant.tb_task (identifier) WHERE deleted_at IS NULL;
CREATE INDEX idx_task_project_status ON tenant.tb_task (pk_project, status) WHERE deleted_at IS NULL;
CREATE INDEX idx_task_assignee ON tenant.tb_task (assignee_id) WHERE deleted_at IS NULL AND assignee_id IS NOT NULL;
CREATE INDEX idx_task_parent ON tenant.tb_task (parent_task_id) WHERE parent_task_id IS NOT NULL;
CREATE INDEX idx_task_due_date ON tenant.tb_task (due_date) WHERE deleted_at IS NULL AND due_date IS NOT NULL;

-- Document indexes
CREATE UNIQUE INDEX idx_document_identifier ON tenant.tb_document (identifier) WHERE deleted_at IS NULL;
CREATE INDEX idx_document_organization ON tenant.tb_document (pk_organization, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_document_content_type ON tenant.tb_document (content_type);

-- Notification indexes
CREATE INDEX idx_notification_user_unread ON tenant.tb_notification (pk_user, created_at DESC) WHERE read_at IS NULL;
CREATE INDEX idx_notification_entity ON tenant.tb_notification (entity_type, entity_id);
CREATE INDEX idx_notification_type ON tenant.tb_notification (notification_type, created_at DESC);

-- JSONB indexes for complex queries
CREATE INDEX idx_organization_data_gin ON tenant.tb_organization USING GIN (data);
CREATE INDEX idx_user_data_gin ON tenant.tb_user USING GIN (data);
CREATE INDEX idx_project_data_gin ON tenant.tb_project USING GIN (data);
CREATE INDEX idx_task_data_gin ON tenant.tb_task USING GIN (data);

-- Specific JSONB path indexes for common queries
CREATE INDEX idx_user_roles ON tenant.tb_user USING GIN ((data->'roles'));
CREATE INDEX idx_project_tags ON tenant.tb_project USING GIN ((data->'tags'));
CREATE INDEX idx_task_labels ON tenant.tb_task USING GIN ((data->'labels'));

-- Foreign key constraints for referential integrity
ALTER TABLE tenant.tb_task ADD CONSTRAINT fk_task_parent
    FOREIGN KEY (parent_task_id) REFERENCES tenant.tb_task(pk_task);

-- Constraints for business rules
ALTER TABLE tenant.tb_organization ADD CONSTRAINT chk_organization_data_required
    CHECK (data ? 'name' AND data ? 'legal_name');

ALTER TABLE tenant.tb_user ADD CONSTRAINT chk_user_data_required
    CHECK (data ? 'email' AND data ? 'first_name' AND data ? 'last_name');

ALTER TABLE tenant.tb_project ADD CONSTRAINT chk_project_data_required
    CHECK (data ? 'name' AND data ? 'owner_id' AND data ? 'status');

ALTER TABLE tenant.tb_task ADD CONSTRAINT chk_task_data_required
    CHECK (data ? 'title' AND data ? 'status');

-- Check constraints for data integrity
ALTER TABLE tenant.tb_user ADD CONSTRAINT chk_user_version_positive
    CHECK (version > 0);

ALTER TABLE tenant.tb_project ADD CONSTRAINT chk_project_version_positive
    CHECK (version > 0);

ALTER TABLE tenant.tb_task ADD CONSTRAINT chk_task_version_positive
    CHECK (version > 0);

-- Ensure audit trail completeness
ALTER TABLE tenant.tb_organization ADD CONSTRAINT chk_organization_audit_complete
    CHECK (created_by IS NOT NULL AND updated_by IS NOT NULL);

ALTER TABLE tenant.tb_user ADD CONSTRAINT chk_user_audit_complete
    CHECK (created_by IS NOT NULL AND updated_by IS NOT NULL);

-- Comments for documentation
COMMENT ON TABLE audit_events IS 'Unified audit table with CDC + cryptographic chain integrity';
COMMENT ON TABLE tenant.tb_organization IS 'Multi-tenant organizations with enterprise features';
COMMENT ON TABLE tenant.tb_user IS 'Users with comprehensive audit trail and role management';
COMMENT ON TABLE tenant.tb_project IS 'Projects with complex business logic and timeline management';
COMMENT ON TABLE tenant.tb_task IS 'Tasks with nested relationships and workload tracking';
COMMENT ON TABLE tenant.tb_document IS 'Document versioning and file management';
COMMENT ON TABLE tenant.tb_notification IS 'Event-driven notification system';

-- Grant appropriate permissions
CREATE ROLE enterprise_api_role;

GRANT USAGE ON SCHEMA tenant TO enterprise_api_role;
GRANT USAGE ON SCHEMA app TO enterprise_api_role;
GRANT USAGE ON SCHEMA core TO enterprise_api_role;

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA tenant TO enterprise_api_role;
GRANT SELECT ON ALL SEQUENCES IN SCHEMA tenant TO enterprise_api_role;
