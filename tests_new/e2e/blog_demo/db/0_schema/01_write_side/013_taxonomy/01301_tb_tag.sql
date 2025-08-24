-- Tags command table (tb_tag)
-- Stores hierarchical tags and categories

CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    identifier CITEXT UNIQUE NOT NULL, -- Business identifier (slug)
    fk_parent_tag UUID, -- Self-reference for hierarchy

    -- Flat normalized columns
    name CITEXT NOT NULL,
    description TEXT,
    color VARCHAR(7), -- Hex color code
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT slug_format CHECK (identifier ~* '^[a-z0-9-]+$'),
    CONSTRAINT slug_length CHECK (length(identifier) >= 1 AND length(identifier) <= 50),
    CONSTRAINT name_length CHECK (length(name) >= 1 AND length(name) <= 50),
    CONSTRAINT color_format CHECK (color IS NULL OR color ~* '^#[0-9a-f]{6}$'),
    CONSTRAINT no_self_parent CHECK (pk_tag != fk_parent_tag),

    -- Foreign key
    CONSTRAINT fk_tag_parent FOREIGN KEY (fk_parent_tag) REFERENCES tb_tag(pk_tag) ON DELETE SET NULL
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_tag_identifier ON tb_tag(identifier);
CREATE INDEX IF NOT EXISTS idx_tb_tag_parent ON tb_tag(fk_parent_tag);
CREATE INDEX IF NOT EXISTS idx_tb_tag_created_at ON tb_tag(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_tag_pk_tag ON tb_tag(pk_tag);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_tag_name ON tb_tag(name);
CREATE INDEX IF NOT EXISTS idx_tb_tag_is_active ON tb_tag(is_active);
CREATE INDEX IF NOT EXISTS idx_tb_tag_sort_order ON tb_tag(sort_order);
CREATE INDEX IF NOT EXISTS idx_tb_tag_active_sort ON tb_tag(is_active, sort_order);

-- Audit trigger
CREATE OR REPLACE FUNCTION update_tb_tag_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_tag_updated_at
    BEFORE UPDATE ON tb_tag
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_tag_updated_at();
