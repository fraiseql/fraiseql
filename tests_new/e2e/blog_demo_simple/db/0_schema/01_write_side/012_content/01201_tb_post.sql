-- Posts command table (tb_post)
-- Stores blog post content and metadata

CREATE TABLE IF NOT EXISTS tb_post (
    pk_post UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    identifier CITEXT UNIQUE NOT NULL, -- Business identifier (slug)
    fk_author UUID NOT NULL, -- Reference to tb_user.pk_user

    -- Flat normalized columns
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    status post_status NOT NULL DEFAULT 'draft',
    featured BOOLEAN NOT NULL DEFAULT false,
    published_at TIMESTAMPTZ,

    -- SEO and custom metadata as JSONB
    seo_metadata JSONB DEFAULT '{}',
    custom_fields JSONB DEFAULT '{}',

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT slug_format CHECK (identifier ~* '^[a-z0-9-]+$'),
    CONSTRAINT slug_length CHECK (length(identifier) >= 1 AND length(identifier) <= 200),
    CONSTRAINT title_length CHECK (length(title) >= 1 AND length(title) <= 200),
    CONSTRAINT published_at_logic CHECK (
        (status = 'published' AND published_at IS NOT NULL) OR
        (status != 'published')
    ),

    -- Foreign key
    CONSTRAINT fk_post_author FOREIGN KEY (fk_author) REFERENCES tb_user(pk_user) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX IF NOT EXISTS idx_tb_post_author ON tb_post(fk_author);
CREATE INDEX IF NOT EXISTS idx_tb_post_created_at ON tb_post(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_post_pk_post ON tb_post(pk_post);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_status ON tb_post(status);
CREATE INDEX IF NOT EXISTS idx_tb_post_featured ON tb_post(featured);
CREATE INDEX IF NOT EXISTS idx_tb_post_published_at ON tb_post(published_at DESC);
CREATE INDEX IF NOT EXISTS idx_tb_post_status_published_at ON tb_post(status, published_at DESC);

-- Full-text search indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_title_gin ON tb_post USING gin(to_tsvector('english', title));
CREATE INDEX IF NOT EXISTS idx_tb_post_content_gin ON tb_post USING gin(to_tsvector('english', content));
CREATE INDEX IF NOT EXISTS idx_tb_post_search_gin ON tb_post USING gin(
    to_tsvector('english', title || ' ' || COALESCE(content, '') || ' ' || COALESCE(excerpt, ''))
);

-- JSONB indexes for metadata
CREATE INDEX IF NOT EXISTS idx_tb_post_seo_metadata_gin ON tb_post USING GIN (seo_metadata);
CREATE INDEX IF NOT EXISTS idx_tb_post_custom_fields_gin ON tb_post USING GIN (custom_fields);

-- Audit trigger
CREATE OR REPLACE FUNCTION update_tb_post_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_post_updated_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_post_updated_at();
