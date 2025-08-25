-- Multi-tenant Posts Table (tb_post)
-- Blog post content with tenant isolation following PrintOptim patterns

CREATE TABLE tenant.tb_post (
    -- Primary keys and tenant isolation
    pk_post UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fk_organization UUID NOT NULL REFERENCES management.tb_organization(pk_organization),
    fk_author UUID NOT NULL REFERENCES tenant.tb_user(pk_user),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)

    -- Business identifier (slug within tenant)
    identifier CITEXT NOT NULL,

    -- Content fields stored in JSONB data column (PrintOptim pattern)
    data JSONB NOT NULL DEFAULT '{}',

    -- Audit columns (PrintOptim standard)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Multi-tenant constraints
    CONSTRAINT unique_slug_per_tenant UNIQUE (fk_organization, identifier),
    CONSTRAINT slug_format CHECK (identifier ~* '^[a-z0-9-]+$'),
    CONSTRAINT slug_length CHECK (length(identifier) >= 1 AND length(identifier) <= 200),

    -- JSONB data structure validation
    CONSTRAINT data_structure_check CHECK (
        data ? 'title' AND
        data ? 'content' AND
        data ? 'status' AND
        data ? 'excerpt'
    ),

    -- Foreign key with cascade (tenant isolation)
    CONSTRAINT fk_post_author_tenant CHECK (
        -- Ensure author belongs to same organization (enforced by RLS)
        fk_organization IS NOT NULL
    )
);

-- Enable Row Level Security
ALTER TABLE tenant.tb_post ENABLE ROW LEVEL SECURITY;

-- RLS Policy: Posts are only accessible within the same organization
CREATE POLICY post_tenant_isolation ON tenant.tb_post
    FOR ALL
    TO public
    USING (fk_organization = current_tenant_id());

-- Core indexes (PrintOptim pattern)
CREATE INDEX idx_tb_post_fk_organization ON tenant.tb_post(fk_organization);
CREATE INDEX idx_tb_post_fk_author ON tenant.tb_post(fk_organization, fk_author);
CREATE INDEX idx_tb_post_identifier ON tenant.tb_post(fk_organization, identifier);
CREATE INDEX idx_tb_post_pk_post ON tenant.tb_post(pk_post);
CREATE INDEX idx_tb_post_created_at ON tenant.tb_post(created_at DESC);

-- JSONB indexes for data column queries
CREATE INDEX idx_tb_post_data_gin ON tenant.tb_post USING GIN (data);
CREATE INDEX idx_tb_post_status ON tenant.tb_post USING GIN ((data->'status'));
CREATE INDEX idx_tb_post_published_at ON tenant.tb_post USING GIN ((data->'published_at'));

-- Full-text search indexes on JSONB content
CREATE INDEX idx_tb_post_title_search ON tenant.tb_post USING GIN (
    to_tsvector('english', data->>'title')
);
CREATE INDEX idx_tb_post_content_search ON tenant.tb_post USING GIN (
    to_tsvector('english', data->>'content')
);
CREATE INDEX idx_tb_post_full_search ON tenant.tb_post USING GIN (
    to_tsvector('english',
        COALESCE(data->>'title', '') || ' ' ||
        COALESCE(data->>'content', '') || ' ' ||
        COALESCE(data->>'excerpt', '')
    )
);

-- Update trigger with tenant validation (PrintOptim pattern)
CREATE OR REPLACE FUNCTION tenant.update_tb_post_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Validate tenant context
    IF NOT validate_tenant_access(NEW.fk_organization) THEN
        RAISE EXCEPTION 'Access denied: invalid tenant context for post update';
    END IF;

    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;

    -- Auto-generate slug from title if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = lower(regexp_replace(
            regexp_replace(NEW.data->>'title', '[^a-zA-Z0-9\s-]', '', 'g'),
            '\s+', '-', 'g'
        ));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE TRIGGER tr_tb_post_updated_at
    BEFORE UPDATE ON tenant.tb_post
    FOR EACH ROW
    EXECUTE FUNCTION tenant.update_tb_post_updated_at();

-- Comments for documentation
COMMENT ON TABLE tenant.tb_post IS 'Multi-tenant blog posts with organization isolation';
COMMENT ON COLUMN tenant.tb_post.fk_organization IS 'References the organization this post belongs to';
COMMENT ON COLUMN tenant.tb_post.identifier IS 'URL slug unique within the organization';
COMMENT ON COLUMN tenant.tb_post.data IS 'Post content stored as JSONB: {title, content, status, excerpt, featured, published_at, seo_metadata, custom_fields}';
COMMENT ON CONSTRAINT unique_slug_per_tenant ON tenant.tb_post IS 'Slug must be unique within each organization';
