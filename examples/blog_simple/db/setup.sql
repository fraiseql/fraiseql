-- FraiseQL Blog Simple - Database Schema with Trinity Pattern
-- Complete PostgreSQL setup for blog application following FraiseQL v1 patterns

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Drop existing tables if they exist (for clean setup)
DROP TABLE IF EXISTS post_tags;
DROP TABLE IF EXISTS tb_comment;
DROP TABLE IF EXISTS tb_post;
DROP TABLE IF EXISTS tb_tag;
DROP TABLE IF EXISTS tb_user;

-- ==============================================================================
-- BASE TABLES (tb_*): Normalized, write-optimized, source of truth
-- ==============================================================================

-- Users table - Trinity Pattern
CREATE TABLE tb_user (
    -- Trinity Identifiers
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
    -- Trinity Identifiers
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
    -- Trinity Identifiers
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
    -- Trinity Identifiers
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
-- INDEXES: Performance optimization using internal pk_* columns
-- ==============================================================================

-- User indexes
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier);
CREATE INDEX idx_tb_user_role ON tb_user(role);
CREATE INDEX idx_tb_user_id ON tb_user(id);  -- For UUID lookups from GraphQL

-- Tag indexes
CREATE INDEX idx_tb_tag_identifier ON tb_tag(identifier);
CREATE INDEX idx_tb_tag_name ON tb_tag(name);
CREATE INDEX idx_tb_tag_id ON tb_tag(id);  -- For UUID lookups from GraphQL

-- Post indexes (using INT foreign keys for performance)
CREATE INDEX idx_tb_post_fk_author ON tb_post(fk_author);
CREATE INDEX idx_tb_post_status ON tb_post(status);
CREATE INDEX idx_tb_post_published_at ON tb_post(published_at) WHERE published_at IS NOT NULL;
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_id ON tb_post(id);  -- For UUID lookups from GraphQL
CREATE INDEX idx_tb_post_title_search ON tb_post USING GIN (to_tsvector('english', title));
CREATE INDEX idx_tb_post_content_search ON tb_post USING GIN (to_tsvector('english', content));

-- Comment indexes (using INT foreign keys for performance)
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);
CREATE INDEX idx_tb_comment_fk_author ON tb_comment(fk_author);
CREATE INDEX idx_tb_comment_fk_parent ON tb_comment(fk_parent) WHERE fk_parent IS NOT NULL;
CREATE INDEX idx_tb_comment_status ON tb_comment(status);
CREATE INDEX idx_tb_comment_id ON tb_comment(id);  -- For UUID lookups from GraphQL

-- Post tags indexes
CREATE INDEX idx_post_tags_fk_tag ON post_tags(fk_tag);

-- ==============================================================================
-- TRIGGERS: Automated updates
-- ==============================================================================

-- Trigger for updated_at
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

-- Trigger to set published_at when status changes to published
CREATE OR REPLACE FUNCTION set_published_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Set published_at when status changes to published
    IF NEW.status = 'published' AND (OLD.status != 'published' OR NEW.published_at IS NULL) THEN
        NEW.published_at = NOW();
    END IF;

    -- Clear published_at when status changes away from published
    IF NEW.status != 'published' AND OLD.status = 'published' THEN
        NEW.published_at = NULL;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_post_set_published_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION set_published_at();

-- ==============================================================================
-- HELPER FUNCTIONS: Slug generation and utilities
-- ==============================================================================

-- Function to generate slug from title
CREATE OR REPLACE FUNCTION generate_slug(input_text TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN lower(regexp_replace(
        regexp_replace(input_text, '[^a-zA-Z0-9\s]', '', 'g'),
        '\s+', '-', 'g'
    ));
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-generate slug from title for posts
CREATE OR REPLACE FUNCTION auto_generate_post_slug()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate identifier (slug) from title if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = generate_slug(NEW.title);

        -- Ensure uniqueness by appending part of UUID
        WHILE EXISTS (SELECT 1 FROM tb_post WHERE identifier = NEW.identifier AND pk_post != COALESCE(NEW.pk_post, -1)) LOOP
            NEW.identifier = NEW.identifier || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_post_auto_generate_slug
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION auto_generate_post_slug();

-- Trigger to auto-generate slug from name for tags
CREATE OR REPLACE FUNCTION auto_generate_tag_slug()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate identifier (slug) from name if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = generate_slug(NEW.name);

        -- Ensure uniqueness by appending part of UUID
        WHILE EXISTS (SELECT 1 FROM tb_tag WHERE identifier = NEW.identifier AND pk_tag != COALESCE(NEW.pk_tag, -1)) LOOP
            NEW.identifier = NEW.identifier || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_tag_auto_generate_slug
    BEFORE INSERT OR UPDATE ON tb_tag
    FOR EACH ROW EXECUTE FUNCTION auto_generate_tag_slug();

-- ==============================================================================
-- SECURITY: Row Level Security (RLS) examples
-- ==============================================================================

ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_post ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_comment ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own data
CREATE POLICY user_own_data ON tb_user
    FOR ALL
    USING (id = current_setting('app.current_user_id', true)::uuid);

-- Policy: Published posts are visible to all, drafts only to author
CREATE POLICY posts_visibility ON tb_post
    FOR SELECT
    USING (
        status = 'published'
        OR EXISTS (
            SELECT 1 FROM tb_user
            WHERE tb_user.pk_user = tb_post.fk_author
            AND tb_user.id = current_setting('app.current_user_id', true)::uuid
        )
    );

-- Policy: Users can insert their own posts
CREATE POLICY posts_insert ON tb_post
    FOR INSERT
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM tb_user
            WHERE tb_user.pk_user = fk_author
            AND tb_user.id = current_setting('app.current_user_id', true)::uuid
        )
    );

-- Policy: Users can update their own posts
CREATE POLICY posts_update ON tb_post
    FOR UPDATE
    USING (
        EXISTS (
            SELECT 1 FROM tb_user
            WHERE tb_user.pk_user = tb_post.fk_author
            AND tb_user.id = current_setting('app.current_user_id', true)::uuid
        )
    );

-- Policy: Approved comments are visible to all
CREATE POLICY comments_visibility ON tb_comment
    FOR SELECT
    USING (status = 'approved');

-- ==============================================================================
-- VIEWS: Backward compatibility for GraphQL models
-- ==============================================================================

-- Create views that follow Trinity pattern: expose only id (UUID) fields, not pk_/fk_
-- Views with UUID relationships exposed (Trinity pattern compliant)
CREATE VIEW v_users AS
SELECT
    id,
    identifier,
    identifier AS username,  -- Use identifier as username since table only has identifier
    email,
    password_hash,
    role,
    profile_data,
    created_at,
    updated_at
FROM tb_user;

CREATE VIEW v_posts AS
SELECT
    p.id,
    p.identifier,
    p.identifier AS slug,  -- Use identifier as slug since table only has identifier
    p.title,
    p.content,
    p.excerpt,
    p.status,
    p.published_at,
    p.created_at,
    p.updated_at,
    u.id AS author_id  -- ✅ UUID relationship from JOIN
FROM tb_post p
JOIN tb_user u ON p.fk_author = u.pk_user;

CREATE VIEW v_comments AS
SELECT
    c.id,
    c.identifier,
    c.content,
    c.status,
    c.created_at,
    c.updated_at,
    p.id AS post_id,       -- ✅ UUID relationship from JOIN
    u.id AS author_id,     -- ✅ UUID relationship from JOIN
    pc.id AS parent_id     -- ✅ UUID relationship from JOIN
FROM tb_comment c
JOIN tb_post p ON c.fk_post = p.pk_post
JOIN tb_user u ON c.fk_author = u.pk_user
LEFT JOIN tb_comment pc ON c.fk_parent = pc.pk_comment;

CREATE VIEW v_tags AS
SELECT
    id,
    identifier,
    name,
    identifier AS slug,  -- Use identifier as slug since table only has identifier
    color,
    description,
    created_at
FROM tb_tag;

-- ==============================================================================
-- PERMISSIONS: Grant basic permissions
-- ==============================================================================

-- Note: In production, create dedicated roles with minimal permissions
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO PUBLIC;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO PUBLIC;

-- ==============================================================================
-- COMMENTS: Documentation for Trinity Pattern
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
