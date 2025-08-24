-- Comments command table (tb_comment)
-- Stores comment data with nested threading support

CREATE TABLE IF NOT EXISTS tb_comment (
    pk_comment UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    fk_post UUID NOT NULL, -- Reference to tb_post.pk_post
    fk_author UUID NOT NULL, -- Reference to tb_user.pk_user
    fk_parent_comment UUID, -- Self-reference for threading

    -- Flat normalized columns
    content TEXT NOT NULL,
    status comment_status NOT NULL DEFAULT 'pending',

    -- Moderation metadata as JSONB
    moderation_data JSONB DEFAULT '{}',

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT content_length CHECK (length(content) >= 1 AND length(content) <= 2000),
    CONSTRAINT no_self_parent CHECK (pk_comment != fk_parent_comment),

    -- Foreign keys
    CONSTRAINT fk_comment_post FOREIGN KEY (fk_post) REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    CONSTRAINT fk_comment_author FOREIGN KEY (fk_author) REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    CONSTRAINT fk_comment_parent FOREIGN KEY (fk_parent_comment) REFERENCES tb_comment(pk_comment) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_comment_post ON tb_comment(fk_post);
CREATE INDEX IF NOT EXISTS idx_tb_comment_author ON tb_comment(fk_author);
CREATE INDEX IF NOT EXISTS idx_tb_comment_parent ON tb_comment(fk_parent_comment);
CREATE INDEX IF NOT EXISTS idx_tb_comment_created_at ON tb_comment(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_comment_pk_comment ON tb_comment(pk_comment);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_comment_status ON tb_comment(status);
CREATE INDEX IF NOT EXISTS idx_tb_comment_post_status ON tb_comment(fk_post, status);

-- JSONB indexes for moderation data
CREATE INDEX IF NOT EXISTS idx_tb_comment_moderation_data_gin ON tb_comment USING GIN (moderation_data);

-- Audit trigger
CREATE OR REPLACE FUNCTION update_tb_comment_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_comment_updated_at
    BEFORE UPDATE ON tb_comment
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_comment_updated_at();
