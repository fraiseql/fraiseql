-- FraiseQL Blog Simple - Database Schema with Trinity Identifiers & CQRS
-- Complete PostgreSQL setup with Trinity pattern:
--   pk_* (INT IDENTITY) - Internal primary key for fast joins
--   id (UUID)           - Public API identifier (secure)
--   identifier (TEXT)   - Human-readable URL slug
--
-- Naming conventions:
--   tb_*  - Command-side tables (normalized write model) - SINGULAR
--   tv_*  - Query-side views (denormalized read model with JSONB) - SINGULAR
--   pk_*  - Primary keys (GENERATED ALWAYS AS IDENTITY)
--   fk_*  - Foreign keys (references pk_*)

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Drop existing tables if they exist (for clean setup)
DROP TABLE IF EXISTS tb_post_tag CASCADE;
DROP TABLE IF EXISTS tb_comment CASCADE;
DROP TABLE IF EXISTS tb_post CASCADE;
DROP TABLE IF EXISTS tb_tag CASCADE;
DROP TABLE IF EXISTS tb_user CASCADE;

-- ============================================================================
-- tb_user - Command-side user table with Trinity
-- ============================================================================
CREATE TABLE tb_user (
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

COMMENT ON TABLE tb_user IS 'Command-side user table (normalized write model)';
COMMENT ON COLUMN tb_user.pk_user IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN tb_user.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN tb_user.identifier IS 'Human-readable username for URLs (e.g., @johndoe)';

-- ============================================================================
-- tb_tag - Command-side tag table with Trinity
-- ============================================================================
CREATE TABLE tb_tag (
    pk_tag INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    identifier TEXT UNIQUE,  -- slug as identifier (tech, python, etc.)
    name TEXT NOT NULL UNIQUE CHECK (length(name) >= 1),
    slug TEXT NOT NULL UNIQUE CHECK (length(slug) >= 1),
    color TEXT DEFAULT '#6366f1' CHECK (color ~ '^#[0-9A-Fa-f]{6}$'),
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_tag IS 'Command-side tag table (normalized write model)';
COMMENT ON COLUMN tb_tag.pk_tag IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN tb_tag.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN tb_tag.identifier IS 'Human-readable slug for URLs (e.g., /tags/python)';

-- ============================================================================
-- tb_post - Command-side post table with Trinity
-- ============================================================================
CREATE TABLE tb_post (
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    identifier TEXT UNIQUE,  -- slug as identifier
    title TEXT NOT NULL CHECK (length(title) >= 1),
    slug TEXT NOT NULL UNIQUE CHECK (length(slug) >= 1),
    content TEXT NOT NULL CHECK (length(content) >= 1),
    excerpt TEXT,
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_post IS 'Command-side post table (normalized write model)';
COMMENT ON COLUMN tb_post.pk_post IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN tb_post.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN tb_post.identifier IS 'Human-readable slug for URLs (e.g., /posts/my-first-post)';
COMMENT ON COLUMN tb_post.fk_author IS 'Foreign key to tb_user (INT for fast joins)';

-- ============================================================================
-- tb_comment - Command-side comment table with Trinity
-- ============================================================================
CREATE TABLE tb_comment (
    pk_comment INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL DEFAULT uuid_generate_v4() UNIQUE,
    fk_post INT NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    fk_parent INT REFERENCES tb_comment(pk_comment) ON DELETE CASCADE,
    content TEXT NOT NULL CHECK (length(content) >= 1),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_comment IS 'Command-side comment table (normalized write model)';
COMMENT ON COLUMN tb_comment.pk_comment IS 'Internal IDENTITY primary key for fast joins (not exposed in API)';
COMMENT ON COLUMN tb_comment.id IS 'Public UUID identifier exposed in GraphQL API';
COMMENT ON COLUMN tb_comment.fk_post IS 'Foreign key to tb_post (INT for fast joins)';
COMMENT ON COLUMN tb_comment.fk_author IS 'Foreign key to tb_user (INT for fast joins)';
COMMENT ON COLUMN tb_comment.fk_parent IS 'Foreign key to parent comment (INT for fast joins)';

-- ============================================================================
-- tb_post_tag - Many-to-many junction table (using INT foreign keys)
-- ============================================================================
CREATE TABLE tb_post_tag (
    fk_post INT NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    fk_tag INT NOT NULL REFERENCES tb_tag(pk_tag) ON DELETE CASCADE,
    PRIMARY KEY (fk_post, fk_tag)
);

COMMENT ON TABLE tb_post_tag IS 'Junction table using INT foreign keys for fast joins';

-- ============================================================================
-- Indexes for performance
-- ============================================================================

-- tb_user indexes
CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier) WHERE identifier IS NOT NULL;
CREATE INDEX idx_tb_user_username ON tb_user(username);
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_role ON tb_user(role);

-- tb_tag indexes
CREATE INDEX idx_tb_tag_id ON tb_tag(id);
CREATE INDEX idx_tb_tag_identifier ON tb_tag(identifier);
CREATE INDEX idx_tb_tag_slug ON tb_tag(slug);
CREATE INDEX idx_tb_tag_name ON tb_tag(name);

-- tb_post indexes
CREATE INDEX idx_tb_post_id ON tb_post(id);
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_fk_author ON tb_post(fk_author);
CREATE INDEX idx_tb_post_status ON tb_post(status);
CREATE INDEX idx_tb_post_published_at ON tb_post(published_at) WHERE published_at IS NOT NULL;
CREATE INDEX idx_tb_post_slug ON tb_post(slug);
CREATE INDEX idx_tb_post_title_search ON tb_post USING GIN (to_tsvector('english', title));
CREATE INDEX idx_tb_post_content_search ON tb_post USING GIN (to_tsvector('english', content));

-- tb_comment indexes
CREATE INDEX idx_tb_comment_id ON tb_comment(id);
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);
CREATE INDEX idx_tb_comment_fk_author ON tb_comment(fk_author);
CREATE INDEX idx_tb_comment_fk_parent ON tb_comment(fk_parent) WHERE fk_parent IS NOT NULL;
CREATE INDEX idx_tb_comment_status ON tb_comment(status);

-- tb_post_tag indexes
CREATE INDEX idx_tb_post_tag_fk_tag ON tb_post_tag(fk_tag);

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

CREATE TRIGGER update_tb_user_updated_at
    BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_post_updated_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_comment_updated_at
    BEFORE UPDATE ON tb_comment
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

CREATE TRIGGER tb_post_set_published_at
    BEFORE UPDATE ON tb_post
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
            SELECT 1 FROM tb_user
            WHERE identifier = NEW.identifier
            AND pk_user != COALESCE(NEW.pk_user, 0)
        ) LOOP
            NEW.identifier = NEW.identifier || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_user_auto_generate_identifier
    BEFORE INSERT OR UPDATE ON tb_user
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
            SELECT 1 FROM tb_post
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

CREATE TRIGGER tb_post_auto_generate_identifier
    BEFORE INSERT OR UPDATE ON tb_post
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
            SELECT 1 FROM tb_tag
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

CREATE TRIGGER tb_tag_auto_generate_identifier
    BEFORE INSERT OR UPDATE ON tb_tag
    FOR EACH ROW EXECUTE FUNCTION auto_generate_tag_identifier();

-- ============================================================================
-- Security: Row Level Security (RLS) examples
-- ============================================================================
-- Enable RLS for sensitive operations
ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_post ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_comment ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own data
CREATE POLICY user_own_data ON tb_user
    FOR ALL
    USING (pk_user = current_setting('app.current_user_pk')::int);

-- Policy: Published posts are visible to all, drafts only to author
CREATE POLICY post_visibility ON tb_post
    FOR SELECT
    USING (
        status = 'published'
        OR fk_author = current_setting('app.current_user_pk')::int
    );

-- Policy: Users can insert their own posts
CREATE POLICY post_insert ON tb_post
    FOR INSERT
    WITH CHECK (fk_author = current_setting('app.current_user_pk')::int);

-- Policy: Users can update their own posts
CREATE POLICY post_update ON tb_post
    FOR UPDATE
    USING (fk_author = current_setting('app.current_user_pk')::int);

-- Policy: Approved comments are visible to all
CREATE POLICY comment_visibility ON tb_comment
    FOR SELECT
    USING (status = 'approved');

-- Grant basic permissions
-- Note: In production, create dedicated roles with minimal permissions
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO PUBLIC;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO PUBLIC;
