-- Blog API Schema with Trinity Pattern
-- Follows FraiseQL v1 conventions for optimal performance

-- Create extension for UUID generation if not exists
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ==============================================================================
-- COMMAND SIDE: Base tables (tb_*) with Trinity Pattern
-- ==============================================================================

-- User table (write side) - Trinity Pattern
CREATE TABLE IF NOT EXISTS tb_user (
    -- Trinity Identifiers
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,         -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                           -- Human-readable (email/username)

    -- User data
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Post table (write side) - Trinity Pattern with INT foreign keys
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

-- Comment table (write side) - Trinity Pattern with INT foreign keys
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

-- ==============================================================================
-- INDEXES: Performance optimization using internal pk_* and fk_* INT columns
-- ==============================================================================

CREATE INDEX idx_tb_user_id ON tb_user(id);  -- For UUID lookups from GraphQL
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier);
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_active ON tb_user(is_active);

CREATE INDEX idx_tb_post_id ON tb_post(id);  -- For UUID lookups from GraphQL
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_fk_user ON tb_post(fk_user);  -- Fast INT FK index
CREATE INDEX idx_tb_post_slug ON tb_post(slug);
CREATE INDEX idx_tb_post_published ON tb_post(is_published);
CREATE INDEX idx_tb_post_created ON tb_post(created_at);
CREATE INDEX idx_tb_post_tags ON tb_post USING gin(tags);

CREATE INDEX idx_tb_comment_id ON tb_comment(id);  -- For UUID lookups from GraphQL
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);  -- Fast INT FK index
CREATE INDEX idx_tb_comment_fk_user ON tb_comment(fk_user);  -- Fast INT FK index
CREATE INDEX idx_tb_comment_fk_parent ON tb_comment(fk_parent_comment);  -- Fast INT FK index

-- ==============================================================================
-- TRIGGERS: Automated updated_at management
-- ==============================================================================

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_tb_user_updated_at
    BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_post_updated_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_comment_updated_at
    BEFORE UPDATE ON tb_comment
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ==============================================================================
-- QUERY SIDE: Views (v_*) for GraphQL queries
-- ==============================================================================

-- User view (base) - Returns JSONB with UUID for GraphQL
CREATE OR REPLACE VIEW v_user AS
SELECT
    u.id,  -- UUID for GraphQL (public API)
    u.identifier,  -- Human-readable identifier
    jsonb_build_object(
        'id', u.id::text,  -- UUID as text for GraphQL
        'identifier', u.identifier,
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'avatarUrl', u.avatar_url,  -- camelCase for GraphQL
        'isActive', u.is_active,    -- camelCase for GraphQL
        'roles', u.roles,
        'createdAt', u.created_at,  -- camelCase for GraphQL
        'updatedAt', u.updated_at   -- camelCase for GraphQL
    ) AS data
FROM tb_user u;

-- Post view composed with author data - Returns JSONB with UUID for GraphQL
CREATE OR REPLACE VIEW v_post AS
SELECT
    p.id,  -- UUID for GraphQL (public API)
    p.identifier,  -- Human-readable slug
    jsonb_build_object(
        'id', p.id::text,  -- UUID as text for GraphQL
        'identifier', p.identifier,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,      -- camelCase for GraphQL
        'publishedAt', p.published_at,      -- camelCase for GraphQL
        'viewCount', p.view_count,          -- camelCase for GraphQL
        'createdAt', p.created_at,          -- camelCase for GraphQL
        'updatedAt', p.updated_at,          -- camelCase for GraphQL
        'author', jsonb_build_object(
            'id', u.id::text,
            'identifier', u.identifier,
            'name', u.name,
            'avatarUrl', u.avatar_url
        )
    ) AS data
FROM tb_post p
JOIN tb_user u ON u.pk_user = p.fk_user;  -- Fast INT join!

-- Comment view composed with author, post and parent data
CREATE OR REPLACE VIEW v_comment AS
WITH RECURSIVE comment_tree AS (
    -- Base case: comments without parents
    SELECT
        c.pk_comment,
        c.id,
        c.identifier,
        c.content,
        c.is_edited,
        c.fk_post,
        c.fk_user,
        c.fk_parent_comment,
        c.created_at,
        c.updated_at,
        0 AS depth,
        ARRAY[c.pk_comment] AS path  -- Use pk_comment (INT) for path
    FROM tb_comment c
    WHERE c.fk_parent_comment IS NULL

    UNION ALL

    -- Recursive case: child comments
    SELECT
        c.pk_comment,
        c.id,
        c.identifier,
        c.content,
        c.is_edited,
        c.fk_post,
        c.fk_user,
        c.fk_parent_comment,
        c.created_at,
        c.updated_at,
        ct.depth + 1,
        ct.path || c.pk_comment  -- Use pk_comment (INT) for path
    FROM tb_comment c
    JOIN comment_tree ct ON ct.pk_comment = c.fk_parent_comment
    WHERE NOT c.pk_comment = ANY(ct.path) -- Prevent cycles using INT
)
SELECT
    c.id,  -- UUID for GraphQL (public API)
    c.identifier,  -- Human-readable identifier (optional)
    jsonb_build_object(
        'id', c.id::text,  -- UUID as text for GraphQL
        'identifier', c.identifier,
        'content', c.content,
        'isEdited', c.is_edited,  -- camelCase for GraphQL
        'depth', c.depth,
        'createdAt', c.created_at,  -- camelCase for GraphQL
        'updatedAt', c.updated_at,  -- camelCase for GraphQL
        'post', jsonb_build_object(
            'id', p.id::text,
            'identifier', p.identifier,
            'title', p.title,
            'slug', p.slug
        ),
        'author', jsonb_build_object(
            'id', u.id::text,
            'identifier', u.identifier,
            'name', u.name,
            'avatarUrl', u.avatar_url
        ),
        'parentComment', CASE
            WHEN pc.id IS NOT NULL THEN jsonb_build_object(
                'id', pc.id::text,
                'content', pc.content,
                'author', jsonb_build_object(
                    'id', pu.id::text,
                    'identifier', pu.identifier,
                    'name', pu.name
                )
            )
            ELSE NULL
        END
    ) AS data
FROM comment_tree c
JOIN tb_post p ON p.pk_post = c.fk_post  -- Fast INT join!
JOIN tb_user u ON u.pk_user = c.fk_user  -- Fast INT join!
LEFT JOIN tb_comment pc ON pc.pk_comment = c.fk_parent_comment  -- Fast INT join!
LEFT JOIN tb_user pu ON pu.pk_user = pc.fk_user  -- Fast INT join!;

-- ==============================================================================
-- COMMENTS: Documentation for Trinity Pattern
-- ==============================================================================

COMMENT ON TABLE tb_user IS 'Users table with Trinity pattern: pk_user (INT, internal), id (UUID, public API), identifier (TEXT, username)';
COMMENT ON TABLE tb_post IS 'Posts table with Trinity pattern: pk_post (INT, internal), id (UUID, public API), identifier (TEXT, slug)';
COMMENT ON TABLE tb_comment IS 'Comments table with Trinity pattern: pk_comment (INT, internal), id (UUID, public API), identifier (TEXT, optional)';

COMMENT ON COLUMN tb_user.pk_user IS 'Internal primary key (INT) for fast database joins - NOT exposed in GraphQL';
COMMENT ON COLUMN tb_user.id IS 'Public UUID identifier for GraphQL API - secure, prevents enumeration';
COMMENT ON COLUMN tb_user.identifier IS 'Human-readable username for SEO-friendly URLs';

COMMENT ON COLUMN tb_post.fk_user IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_post IS 'Foreign key to tb_post.pk_post (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_user IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_parent_comment IS 'Foreign key to tb_comment.pk_comment (INT) - self-referencing for threaded comments';
