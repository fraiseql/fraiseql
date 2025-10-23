-- ==============================================================================
-- BASE TABLES (tb_*): Normalized, write-optimized, source of truth
-- ==============================================================================

-- Users table - Trinity Pattern
CREATE TABLE tb_user (
    -- Sacred Trinity Identifiers
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,     -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                       -- Human-readable (username/slug)

    -- User data
    email TEXT NOT NULL UNIQUE CHECK (email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'author', 'user')),
    profile_data JSONB DEFAULT '{}'::jsonb,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- Constraints
    CONSTRAINT chk_username_length CHECK (length(identifier) >= 3)
);

-- Tags table - Trinity Pattern
CREATE TABLE tb_tag (
    -- Sacred Trinity Identifiers
    pk_tag INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,   -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,     -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                       -- Human-readable (slug)

    -- Tag data
    name TEXT NOT NULL UNIQUE CHECK (length(name) >= 1),
    color TEXT DEFAULT '#6366f1' CHECK (color ~ '^#[0-9A-Fa-f]{6}$'),
    description TEXT,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Posts table - Trinity Pattern with INT foreign keys
CREATE TABLE tb_post (
    -- Sacred Trinity Identifiers
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,     -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                       -- Human-readable (slug)

    -- Post data
    title TEXT NOT NULL CHECK (length(title) >= 1),
    content TEXT NOT NULL CHECK (length(content) >= 1),
    excerpt TEXT,
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,  -- Fast INT FK!
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    published_at TIMESTAMPTZ,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Comments table - Trinity Pattern with INT foreign keys
CREATE TABLE tb_comment (
    -- Sacred Trinity Identifiers
    pk_comment INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,        -- Public API (secure UUID)
    identifier TEXT UNIQUE,                                   -- Optional for comments

    -- Comment data
    fk_post INT NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,       -- Fast INT FK!
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,     -- Fast INT FK!
    fk_parent INT REFERENCES tb_comment(pk_comment) ON DELETE CASCADE,        -- Fast INT FK!
    content TEXT NOT NULL CHECK (length(content) >= 1),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Many-to-many relationship between posts and tags (using INT FKs)
CREATE TABLE post_tags (
    fk_post INT NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    fk_tag INT NOT NULL REFERENCES tb_tag(pk_tag) ON DELETE CASCADE,
    PRIMARY KEY (fk_post, fk_tag)
);

-- ==============================================================================
-- TABLE COMMENTS: Documentation
-- ==============================================================================

COMMENT ON TABLE tb_user IS 'Users table with Trinity pattern: pk_user (INT, internal), id (UUID, public API), identifier (TEXT, username slug)';
COMMENT ON TABLE tb_post IS 'Posts table with Trinity pattern: pk_post (INT, internal), id (UUID, public API), identifier (TEXT, post slug)';
COMMENT ON TABLE tb_tag IS 'Tags table with Trinity pattern: pk_tag (INT, internal), id (UUID, public API), identifier (TEXT, tag slug)';
COMMENT ON TABLE tb_comment IS 'Comments table with Trinity pattern: pk_comment (INT, internal), id (UUID, public API), identifier (TEXT, optional)';

COMMENT ON COLUMN tb_user.pk_user IS 'Internal primary key (INT) for fast database joins - NOT exposed in GraphQL';
COMMENT ON COLUMN tb_user.id IS 'Public UUID identifier for GraphQL API - secure, prevents enumeration';
COMMENT ON COLUMN tb_user.identifier IS 'Human-readable username slug for SEO-friendly URLs';

COMMENT ON COLUMN tb_post.fk_author IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_post IS 'Foreign key to tb_post.pk_post (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_author IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
