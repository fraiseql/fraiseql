-- FraiseQL Blog Simple - Database Schema with Trinity Identifiers
-- Pristine PostgreSQL 10+ implementation with modern IDENTITY syntax
--
-- Trinity pattern:
--   pk_* (INT GENERATED ALWAYS AS IDENTITY) - Internal primary key for fast joins
--   id (UUID)     - Public API identifier (secure)
--   identifier (TEXT) - Human-readable URL slug (for users/posts/tags)

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Drop existing tables if they exist (for clean setup)
DROP TABLE IF EXISTS post_tags;
DROP TABLE IF EXISTS comments;
DROP TABLE IF EXISTS posts;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS users;

-- ============================================================================
-- Users table with Trinity
-- ============================================================================
CREATE TABLE users (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    identifier TEXT UNIQUE,  -- username as URL slug (@johndoe)
    username TEXT NOT NULL UNIQUE CHECK (length(username) >= 3),
    email TEXT NOT NULL UNIQUE CHECK (email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'author', 'user')),
    profile_data JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON COLUMN users.pk_user IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN users.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN users.identifier IS 'Human-readable username for URLs (e.g., @johndoe)';

-- ============================================================================
-- Tags table with Trinity
-- ============================================================================
CREATE TABLE tags (
    pk_tag INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    identifier TEXT UNIQUE,  -- slug as identifier (tech, python, etc.)
    name TEXT NOT NULL UNIQUE CHECK (length(name) >= 1),
    slug TEXT NOT NULL UNIQUE CHECK (length(slug) >= 1),
    color TEXT DEFAULT '#6366f1' CHECK (color ~ '^#[0-9A-Fa-f]{6}$'),
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON COLUMN tags.pk_tag IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN tags.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN tags.identifier IS 'Human-readable slug for URLs (e.g., /tags/python)';

-- ============================================================================
-- Posts table with Trinity
-- ============================================================================
CREATE TABLE posts (
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    identifier TEXT UNIQUE,  -- slug as identifier
    title TEXT NOT NULL CHECK (length(title) >= 1),
    slug TEXT NOT NULL UNIQUE CHECK (length(slug) >= 1),
    content TEXT NOT NULL CHECK (length(content) >= 1),
    excerpt TEXT,
    pk_author INT NOT NULL REFERENCES users(pk_user) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON COLUMN posts.pk_post IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN posts.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN posts.identifier IS 'Human-readable slug for URLs (e.g., /posts/my-first-post)';
COMMENT ON COLUMN posts.pk_author IS 'Internal INT FK to users (smaller index than UUID)';

-- ============================================================================
-- Comments table with Trinity
-- ============================================================================
CREATE TABLE comments (
    pk_comment INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    pk_post INT NOT NULL REFERENCES posts(pk_post) ON DELETE CASCADE,
    pk_author INT NOT NULL REFERENCES users(pk_user) ON DELETE CASCADE,
    pk_parent INT REFERENCES comments(pk_comment) ON DELETE CASCADE,
    content TEXT NOT NULL CHECK (length(content) >= 1),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON COLUMN comments.pk_comment IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN comments.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN comments.pk_post IS 'Internal INT FK to posts';
COMMENT ON COLUMN comments.pk_author IS 'Internal INT FK to users';
COMMENT ON COLUMN comments.pk_parent IS 'Internal INT FK to parent comment';

-- ============================================================================
-- Many-to-many relationship between posts and tags (using INT FKs)
-- ============================================================================
CREATE TABLE post_tags (
    pk_post INT NOT NULL REFERENCES posts(pk_post) ON DELETE CASCADE,
    pk_tag INT NOT NULL REFERENCES tags(pk_tag) ON DELETE CASCADE,
    PRIMARY KEY (pk_post, pk_tag)
);

COMMENT ON TABLE post_tags IS 'Junction table using INT IDENTITY FKs for fast joins';

-- ============================================================================
-- Indexes for performance
-- ============================================================================

-- Users indexes
CREATE INDEX idx_users_id ON users(id);
CREATE INDEX idx_users_identifier ON users(identifier) WHERE identifier IS NOT NULL;
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);

-- Tags indexes
CREATE INDEX idx_tags_id ON tags(id);
CREATE INDEX idx_tags_identifier ON tags(identifier);
CREATE INDEX idx_tags_slug ON tags(slug);
CREATE INDEX idx_tags_name ON tags(name);

-- Posts indexes
CREATE INDEX idx_posts_id ON posts(id);
CREATE INDEX idx_posts_identifier ON posts(identifier);
CREATE INDEX idx_posts_pk_author ON posts(pk_author);  -- SERIAL FK index
CREATE INDEX idx_posts_status ON posts(status);
CREATE INDEX idx_posts_published_at ON posts(published_at) WHERE published_at IS NOT NULL;
CREATE INDEX idx_posts_slug ON posts(slug);
CREATE INDEX idx_posts_title_search ON posts USING GIN (to_tsvector('english', title));
CREATE INDEX idx_posts_content_search ON posts USING GIN (to_tsvector('english', content));

-- Comments indexes
CREATE INDEX idx_comments_id ON comments(id);
CREATE INDEX idx_comments_pk_post ON comments(pk_post);  -- SERIAL FK index
CREATE INDEX idx_comments_pk_author ON comments(pk_author);  -- SERIAL FK index
CREATE INDEX idx_comments_pk_parent ON comments(pk_parent) WHERE pk_parent IS NOT NULL;  -- SERIAL FK index
CREATE INDEX idx_comments_status ON comments(status);

-- Post-tags indexes
CREATE INDEX idx_post_tags_pk_tag ON post_tags(pk_tag);

-- ============================================================================
-- Triggers for updated_at
-- ============================================================================
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_posts_updated_at
    BEFORE UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_comments_updated_at
    BEFORE UPDATE ON comments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Trigger to set published_at when status changes to published
-- ============================================================================
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

CREATE TRIGGER posts_set_published_at
    BEFORE UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION set_published_at();

-- ============================================================================
-- Function to generate slug from text
-- ============================================================================
CREATE OR REPLACE FUNCTION generate_slug(input_text TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN lower(regexp_replace(
        regexp_replace(input_text, '[^a-zA-Z0-9\s]', '', 'g'),
        '\s+', '-', 'g'
    ));
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Trigger to auto-generate identifier from username
-- ============================================================================
CREATE OR REPLACE FUNCTION auto_generate_user_identifier()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate identifier from username if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = LOWER(NEW.username);

        -- Ensure uniqueness
        WHILE EXISTS (
            SELECT 1 FROM users
            WHERE identifier = NEW.identifier
            AND pk_user != COALESCE(NEW.pk_user, 0)
        ) LOOP
            NEW.identifier = NEW.identifier || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_auto_generate_identifier
    BEFORE INSERT OR UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION auto_generate_user_identifier();

-- ============================================================================
-- Trigger to auto-generate post identifier and slug
-- ============================================================================
CREATE OR REPLACE FUNCTION auto_generate_post_identifier()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate slug from title if not provided
    IF NEW.slug IS NULL OR NEW.slug = '' THEN
        NEW.slug = generate_slug(NEW.title);

        -- Ensure uniqueness
        WHILE EXISTS (
            SELECT 1 FROM posts
            WHERE slug = NEW.slug
            AND pk_post != COALESCE(NEW.pk_post, 0)
        ) LOOP
            NEW.slug = NEW.slug || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    -- identifier is same as slug for posts
    NEW.identifier = NEW.slug;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER posts_auto_generate_identifier
    BEFORE INSERT OR UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION auto_generate_post_identifier();

-- ============================================================================
-- Trigger to auto-generate tag identifier from slug
-- ============================================================================
CREATE OR REPLACE FUNCTION auto_generate_tag_identifier()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate slug from name if not provided
    IF NEW.slug IS NULL OR NEW.slug = '' THEN
        NEW.slug = generate_slug(NEW.name);

        -- Ensure uniqueness
        WHILE EXISTS (
            SELECT 1 FROM tags
            WHERE slug = NEW.slug
            AND pk_tag != COALESCE(NEW.pk_tag, 0)
        ) LOOP
            NEW.slug = NEW.slug || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    -- identifier is same as slug for tags
    NEW.identifier = NEW.slug;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tags_auto_generate_identifier
    BEFORE INSERT OR UPDATE ON tags
    FOR EACH ROW EXECUTE FUNCTION auto_generate_tag_identifier();

-- ============================================================================
-- Security: Row Level Security (RLS) examples
-- ============================================================================
-- Enable RLS for sensitive operations
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE posts ENABLE ROW LEVEL SECURITY;
ALTER TABLE comments ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own data
CREATE POLICY user_own_data ON users
    FOR ALL
    USING (pk_user = current_setting('app.current_user_pk')::int);

-- Policy: Published posts are visible to all, drafts only to author
CREATE POLICY posts_visibility ON posts
    FOR SELECT
    USING (
        status = 'published'
        OR pk_author = current_setting('app.current_user_pk')::int
    );

-- Policy: Users can insert their own posts
CREATE POLICY posts_insert ON posts
    FOR INSERT
    WITH CHECK (pk_author = current_setting('app.current_user_pk')::int);

-- Policy: Users can update their own posts
CREATE POLICY posts_update ON posts
    FOR UPDATE
    USING (pk_author = current_setting('app.current_user_pk')::int);

-- Policy: Approved comments are visible to all
CREATE POLICY comments_visibility ON comments
    FOR SELECT
    USING (status = 'approved');

-- Grant basic permissions
-- Note: In production, create dedicated roles with minimal permissions
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO PUBLIC;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO PUBLIC;
