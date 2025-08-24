-- Post-Tag junction table (tb_post_tag)
-- Many-to-many relationship between posts and tags

CREATE TABLE IF NOT EXISTS tb_post_tag (
    fk_post UUID NOT NULL, -- Reference to tb_post.pk_post
    fk_tag UUID NOT NULL, -- Reference to tb_tag.pk_tag

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,

    -- Primary key
    PRIMARY KEY (fk_post, fk_tag),

    -- Foreign keys
    CONSTRAINT fk_post_tag_post FOREIGN KEY (fk_post) REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    CONSTRAINT fk_post_tag_tag FOREIGN KEY (fk_tag) REFERENCES tb_tag(pk_tag) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_tag_post ON tb_post_tag(fk_post);
CREATE INDEX IF NOT EXISTS idx_tb_post_tag_tag ON tb_post_tag(fk_tag);
CREATE INDEX IF NOT EXISTS idx_tb_post_tag_created_at ON tb_post_tag(created_at);
