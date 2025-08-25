-- Multi-tenant Tags Table (tb_tag)
-- Blog taxonomy tags with tenant isolation following PrintOptim patterns

CREATE TABLE tenant.tb_tag (
    -- Primary keys and tenant isolation
    pk_tag UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fk_organization UUID NOT NULL REFERENCES management.tb_organization(pk_organization),
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
    CONSTRAINT unique_tag_per_tenant UNIQUE (fk_organization, identifier),
    CONSTRAINT tag_identifier_format CHECK (identifier ~* '^[a-z0-9-]+$'),
    CONSTRAINT tag_identifier_length CHECK (length(identifier) >= 1 AND length(identifier) <= 100),

    -- JSONB data structure validation
    CONSTRAINT data_structure_check CHECK (
        data ? 'name' AND
        data ? 'description'
    )
);

-- Enable Row Level Security
ALTER TABLE tenant.tb_tag ENABLE ROW LEVEL SECURITY;

-- RLS Policy: Tags are only accessible within the same organization
CREATE POLICY tag_tenant_isolation ON tenant.tb_tag
    FOR ALL
    TO public
    USING (fk_organization = current_tenant_id());

-- Core indexes (PrintOptim pattern)
CREATE INDEX idx_tb_tag_fk_organization ON tenant.tb_tag(fk_organization);
CREATE INDEX idx_tb_tag_identifier ON tenant.tb_tag(fk_organization, identifier);
CREATE INDEX idx_tb_tag_pk_tag ON tenant.tb_tag(pk_tag);
CREATE INDEX idx_tb_tag_created_at ON tenant.tb_tag(created_at DESC);

-- JSONB indexes for data column queries
CREATE INDEX idx_tb_tag_data_gin ON tenant.tb_tag USING GIN (data);
CREATE INDEX idx_tb_tag_name ON tenant.tb_tag USING GIN ((data->'name'));

-- Full-text search index on tag content
CREATE INDEX idx_tb_tag_search ON tenant.tb_tag USING GIN (
    to_tsvector('english',
        COALESCE(data->>'name', '') || ' ' ||
        COALESCE(data->>'description', '')
    )
);

-- Update trigger with tenant validation (PrintOptim pattern)
CREATE OR REPLACE FUNCTION tenant.update_tb_tag_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Validate tenant context
    IF NOT validate_tenant_access(NEW.fk_organization) THEN
        RAISE EXCEPTION 'Access denied: invalid tenant context for tag update';
    END IF;

    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;

    -- Auto-generate slug from name if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = lower(regexp_replace(
            regexp_replace(NEW.data->>'name', '[^a-zA-Z0-9\s-]', '', 'g'),
            '\s+', '-', 'g'
        ));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE TRIGGER tr_tb_tag_updated_at
    BEFORE UPDATE ON tenant.tb_tag
    FOR EACH ROW
    EXECUTE FUNCTION tenant.update_tb_tag_updated_at();

-- Comments for documentation
COMMENT ON TABLE tenant.tb_tag IS 'Multi-tenant blog tags with organization isolation';
COMMENT ON COLUMN tenant.tb_tag.fk_organization IS 'References the organization this tag belongs to';
COMMENT ON COLUMN tenant.tb_tag.identifier IS 'URL slug unique within the organization';
COMMENT ON COLUMN tenant.tb_tag.data IS 'Tag data stored as JSONB: {name, description, color, metadata}';
