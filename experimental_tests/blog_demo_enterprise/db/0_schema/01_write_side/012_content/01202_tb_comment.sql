-- Multi-tenant Comments Table (tb_comment)
-- Blog comments with tenant isolation following PrintOptim patterns

CREATE TABLE tenant.tb_comment (
    -- Primary keys and tenant isolation
    pk_comment UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fk_organization UUID NOT NULL REFERENCES management.tb_organization(pk_organization),
    fk_post UUID NOT NULL REFERENCES tenant.tb_post(pk_post),
    fk_author UUID NOT NULL REFERENCES tenant.tb_user(pk_user),
    fk_parent_comment UUID REFERENCES tenant.tb_comment(pk_comment),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)

    -- Business identifier (auto-generated)
    identifier CITEXT NOT NULL DEFAULT ('comment-' || extract(epoch from now())::text),

    -- Content fields stored in JSONB data column (PrintOptim pattern)
    data JSONB NOT NULL DEFAULT '{}',

    -- Audit columns (PrintOptim standard)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Multi-tenant constraints
    CONSTRAINT unique_comment_identifier_per_tenant UNIQUE (fk_organization, identifier),

    -- JSONB data structure validation
    CONSTRAINT data_structure_check CHECK (
        data ? 'content' AND
        data ? 'status'
    ),

    -- Business logic constraints
    CONSTRAINT parent_comment_tenant_match CHECK (
        fk_parent_comment IS NULL OR
        -- Parent comment must belong to same organization (enforced by RLS)
        fk_organization IS NOT NULL
    ),

    -- Prevent excessive nesting (performance)
    CONSTRAINT reasonable_comment_depth CHECK (
        -- This will be validated in application logic for deep hierarchies
        fk_parent_comment IS NULL OR fk_parent_comment != pk_comment
    )
);

-- Enable Row Level Security
ALTER TABLE tenant.tb_comment ENABLE ROW LEVEL SECURITY;

-- RLS Policy: Comments are only accessible within the same organization
CREATE POLICY comment_tenant_isolation ON tenant.tb_comment
    FOR ALL
    TO public
    USING (fk_organization = current_tenant_id());

-- Core indexes (PrintOptim pattern)
CREATE INDEX idx_tb_comment_fk_organization ON tenant.tb_comment(fk_organization);
CREATE INDEX idx_tb_comment_fk_post ON tenant.tb_comment(fk_organization, fk_post);
CREATE INDEX idx_tb_comment_fk_author ON tenant.tb_comment(fk_organization, fk_author);
CREATE INDEX idx_tb_comment_fk_parent ON tenant.tb_comment(fk_parent_comment) WHERE fk_parent_comment IS NOT NULL;
CREATE INDEX idx_tb_comment_pk_comment ON tenant.tb_comment(pk_comment);
CREATE INDEX idx_tb_comment_created_at ON tenant.tb_comment(created_at DESC);

-- JSONB indexes for data column queries
CREATE INDEX idx_tb_comment_data_gin ON tenant.tb_comment USING GIN (data);
CREATE INDEX idx_tb_comment_status ON tenant.tb_comment USING GIN ((data->'status'));

-- Full-text search index on comment content
CREATE INDEX idx_tb_comment_content_search ON tenant.tb_comment USING GIN (
    to_tsvector('english', data->>'content')
);

-- Hierarchical query optimization
CREATE INDEX idx_tb_comment_hierarchy ON tenant.tb_comment(fk_organization, fk_post, fk_parent_comment, created_at);

-- Update trigger with tenant validation (PrintOptim pattern)
CREATE OR REPLACE FUNCTION tenant.update_tb_comment_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Validate tenant context
    IF NOT validate_tenant_access(NEW.fk_organization) THEN
        RAISE EXCEPTION 'Access denied: invalid tenant context for comment update';
    END IF;

    -- Ensure post and comment belong to same organization
    IF NOT EXISTS (
        SELECT 1 FROM tenant.tb_post p
        WHERE p.pk_post = NEW.fk_post
        AND p.fk_organization = NEW.fk_organization
    ) THEN
        RAISE EXCEPTION 'Comment post must belong to same organization';
    END IF;

    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE TRIGGER tr_tb_comment_updated_at
    BEFORE UPDATE ON tenant.tb_comment
    FOR EACH ROW
    EXECUTE FUNCTION tenant.update_tb_comment_updated_at();

-- Comments for documentation
COMMENT ON TABLE tenant.tb_comment IS 'Multi-tenant blog comments with organization and post isolation';
COMMENT ON COLUMN tenant.tb_comment.fk_organization IS 'References the organization this comment belongs to';
COMMENT ON COLUMN tenant.tb_comment.fk_post IS 'References the post this comment is on';
COMMENT ON COLUMN tenant.tb_comment.fk_parent_comment IS 'References parent comment for threading (nullable)';
COMMENT ON COLUMN tenant.tb_comment.data IS 'Comment content stored as JSONB: {content, status, metadata}';
COMMENT ON CONSTRAINT parent_comment_tenant_match ON tenant.tb_comment IS 'Parent comment must belong to same organization';
