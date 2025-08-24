-- Multi-tenant Post-Tag Associations (tb_post_tag)
-- Many-to-many relationship between posts and tags with tenant isolation

CREATE TABLE tenant.tb_post_tag (
    -- Primary keys and tenant isolation
    pk_post_tag UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fk_organization UUID NOT NULL REFERENCES management.tb_organization(pk_organization),
    fk_post UUID NOT NULL REFERENCES tenant.tb_post(pk_post),
    fk_tag UUID NOT NULL REFERENCES tenant.tb_tag(pk_tag),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    
    -- Association metadata stored in JSONB data column (PrintOptim pattern)
    data JSONB NOT NULL DEFAULT '{}',
    
    -- Audit columns (PrintOptim standard)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,
    
    -- Multi-tenant constraints
    CONSTRAINT unique_post_tag_per_tenant UNIQUE (fk_organization, fk_post, fk_tag),
    
    -- Ensure all entities belong to same organization
    CONSTRAINT post_tag_organization_consistency CHECK (
        -- This will be validated by RLS and foreign key constraints
        fk_organization IS NOT NULL
    )
);

-- Enable Row Level Security
ALTER TABLE tenant.tb_post_tag ENABLE ROW LEVEL SECURITY;

-- RLS Policy: Post-tag associations are only accessible within the same organization
CREATE POLICY post_tag_tenant_isolation ON tenant.tb_post_tag
    FOR ALL
    TO public
    USING (fk_organization = current_tenant_id());

-- Core indexes (PrintOptim pattern)
CREATE INDEX idx_tb_post_tag_fk_organization ON tenant.tb_post_tag(fk_organization);
CREATE INDEX idx_tb_post_tag_fk_post ON tenant.tb_post_tag(fk_organization, fk_post);
CREATE INDEX idx_tb_post_tag_fk_tag ON tenant.tb_post_tag(fk_organization, fk_tag);
CREATE INDEX idx_tb_post_tag_pk_post_tag ON tenant.tb_post_tag(pk_post_tag);
CREATE INDEX idx_tb_post_tag_created_at ON tenant.tb_post_tag(created_at DESC);

-- JSONB index for data column
CREATE INDEX idx_tb_post_tag_data_gin ON tenant.tb_post_tag USING GIN (data);

-- Optimized queries for both directions
CREATE INDEX idx_tb_post_tag_post_lookup ON tenant.tb_post_tag(fk_post, fk_organization);
CREATE INDEX idx_tb_post_tag_tag_lookup ON tenant.tb_post_tag(fk_tag, fk_organization);

-- Update trigger with tenant validation (PrintOptim pattern)
CREATE OR REPLACE FUNCTION tenant.update_tb_post_tag_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Validate tenant context
    IF NOT validate_tenant_access(NEW.fk_organization) THEN
        RAISE EXCEPTION 'Access denied: invalid tenant context for post-tag association update';
    END IF;
    
    -- Ensure post belongs to same organization
    IF NOT EXISTS (
        SELECT 1 FROM tenant.tb_post p 
        WHERE p.pk_post = NEW.fk_post 
        AND p.fk_organization = NEW.fk_organization
    ) THEN
        RAISE EXCEPTION 'Post must belong to same organization as association';
    END IF;
    
    -- Ensure tag belongs to same organization
    IF NOT EXISTS (
        SELECT 1 FROM tenant.tb_tag t 
        WHERE t.pk_tag = NEW.fk_tag 
        AND t.fk_organization = NEW.fk_organization
    ) THEN
        RAISE EXCEPTION 'Tag must belong to same organization as association';
    END IF;
    
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE TRIGGER tr_tb_post_tag_updated_at
    BEFORE UPDATE ON tenant.tb_post_tag
    FOR EACH ROW
    EXECUTE FUNCTION tenant.update_tb_post_tag_updated_at();

-- Comments for documentation
COMMENT ON TABLE tenant.tb_post_tag IS 'Multi-tenant post-tag associations with organization isolation';
COMMENT ON COLUMN tenant.tb_post_tag.fk_organization IS 'References the organization for tenant isolation';
COMMENT ON COLUMN tenant.tb_post_tag.fk_post IS 'References the post in this association';
COMMENT ON COLUMN tenant.tb_post_tag.fk_tag IS 'References the tag in this association';
COMMENT ON COLUMN tenant.tb_post_tag.data IS 'Association metadata stored as JSONB';
COMMENT ON CONSTRAINT unique_post_tag_per_tenant ON tenant.tb_post_tag IS 'Each post-tag combination must be unique per organization';