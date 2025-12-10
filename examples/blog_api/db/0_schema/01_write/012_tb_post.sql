CREATE TABLE IF NOT EXISTS tb_post (
    -- Trinity Identifiers
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,         -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                           -- Human-readable (slug)

    -- Post data
    fk_user INTEGER NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,  -- Fast INT FK!
    title VARCHAR(500) NOT NULL,
    slug VARCHAR(500) NOT NULL UNIQUE,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[] DEFAULT ARRAY[]::TEXT[],
    is_published BOOLEAN DEFAULT false,
    published_at TIMESTAMPTZ,
    view_count INTEGER DEFAULT 0,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_post IS 'Posts table with Trinity pattern: pk_post (INT, internal), id (UUID, public API), identifier (TEXT, slug)';
COMMENT ON COLUMN tb_post.fk_user IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';

CREATE INDEX idx_tb_post_id ON tb_post(id);
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_fk_user ON tb_post(fk_user);
CREATE INDEX idx_tb_post_slug ON tb_post(slug);
CREATE INDEX idx_tb_post_published ON tb_post(is_published);
CREATE INDEX idx_tb_post_created ON tb_post(created_at);
CREATE INDEX idx_tb_post_tags ON tb_post USING gin(tags);
