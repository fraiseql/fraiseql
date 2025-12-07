CREATE TABLE IF NOT EXISTS tb_comment (
    -- Trinity Identifiers
    pk_comment INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,            -- Public API (secure UUID)
    identifier TEXT UNIQUE,                                       -- Optional for comments

    -- Comment data
    fk_post INTEGER NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,      -- Fast INT FK!
    fk_user INTEGER NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,      -- Fast INT FK!
    fk_parent_comment INTEGER REFERENCES tb_comment(pk_comment) ON DELETE CASCADE,  -- Fast INT FK!
    content TEXT NOT NULL,
    is_edited BOOLEAN DEFAULT false,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON COLUMN tb_comment.fk_post IS 'Foreign key to tb_post.pk_post (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_user IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_parent_comment IS 'Foreign key to tb_comment.pk_comment (INT) - self-referencing for threaded comments';

CREATE INDEX idx_tb_comment_id ON tb_comment(id);
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);
CREATE INDEX idx_tb_comment_fk_user ON tb_comment(fk_user);
CREATE INDEX idx_tb_comment_fk_parent ON tb_comment(fk_parent_comment);
