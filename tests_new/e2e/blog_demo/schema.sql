-- FraiseQL Blog Demo Database Schema
-- This schema demonstrates production-ready patterns for a blog application
-- including proper indexing, constraints, and PostgreSQL best practices.

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "citext";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Create custom types
CREATE TYPE user_role AS ENUM ('admin', 'moderator', 'author', 'user', 'guest');
CREATE TYPE post_status AS ENUM ('draft', 'published', 'archived', 'deleted');
CREATE TYPE comment_status AS ENUM ('pending', 'approved', 'rejected', 'spam');

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username CITEXT UNIQUE NOT NULL,
    email CITEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    role user_role NOT NULL DEFAULT 'user',
    is_active BOOLEAN NOT NULL DEFAULT true,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,

    -- Profile data as JSONB for flexibility
    profile JSONB DEFAULT '{}',
    preferences JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',

    -- Constraints
    CONSTRAINT username_length CHECK (length(username) >= 3 AND length(username) <= 30),
    CONSTRAINT email_format CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$')
);

-- Posts table
CREATE TABLE posts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title TEXT NOT NULL,
    slug CITEXT UNIQUE NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status post_status NOT NULL DEFAULT 'draft',
    featured BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,

    -- SEO and custom metadata as JSONB
    seo_metadata JSONB DEFAULT '{}',
    custom_fields JSONB DEFAULT '{}',

    -- Constraints
    CONSTRAINT title_length CHECK (length(title) >= 1 AND length(title) <= 200),
    CONSTRAINT slug_format CHECK (slug ~* '^[a-z0-9-]+$'),
    CONSTRAINT published_at_logic CHECK (
        (status = 'published' AND published_at IS NOT NULL) OR
        (status != 'published')
    )
);

-- Comments table
CREATE TABLE comments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_id UUID REFERENCES comments(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    status comment_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Moderation metadata
    moderation_data JSONB DEFAULT '{}',

    -- Constraints
    CONSTRAINT content_length CHECK (length(content) >= 1 AND length(content) <= 2000),
    CONSTRAINT no_self_parent CHECK (id != parent_id)
);

-- Tags table (hierarchical categories and tags)
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name CITEXT NOT NULL,
    slug CITEXT UNIQUE NOT NULL,
    description TEXT,
    color VARCHAR(7), -- Hex color code
    parent_id UUID REFERENCES tags(id) ON DELETE SET NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT name_length CHECK (length(name) >= 1 AND length(name) <= 50),
    CONSTRAINT slug_format CHECK (slug ~* '^[a-z0-9-]+$'),
    CONSTRAINT color_format CHECK (color IS NULL OR color ~* '^#[0-9a-f]{6}$'),
    CONSTRAINT no_self_parent CHECK (id != parent_id)
);

-- Post-Tag junction table
CREATE TABLE post_tags (
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (post_id, tag_id)
);

-- User sessions for authentication
CREATE TABLE user_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    refresh_token_hash TEXT UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ip_address INET,
    user_agent TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true
);

-- Post views for analytics
CREATE TABLE post_views (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    viewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ip_address INET,
    user_agent TEXT,
    referrer TEXT
);

-- Notifications system
CREATE TABLE notifications (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT,
    data JSONB DEFAULT '{}',
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT title_length CHECK (length(title) >= 1 AND length(title) <= 100)
);

-- Create indexes for performance
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_users_created_at ON users(created_at);
CREATE INDEX idx_users_is_active ON users(is_active);

CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_posts_status ON posts(status);
CREATE INDEX idx_posts_slug ON posts(slug);
CREATE INDEX idx_posts_published_at ON posts(published_at DESC);
CREATE INDEX idx_posts_featured ON posts(featured);
CREATE INDEX idx_posts_created_at ON posts(created_at DESC);
CREATE INDEX idx_posts_status_published_at ON posts(status, published_at DESC);

-- Full-text search indexes
CREATE INDEX idx_posts_title_gin ON posts USING gin(to_tsvector('english', title));
CREATE INDEX idx_posts_content_gin ON posts USING gin(to_tsvector('english', content));
CREATE INDEX idx_posts_search_gin ON posts USING gin(
    to_tsvector('english', title || ' ' || COALESCE(content, '') || ' ' || COALESCE(excerpt, ''))
);

CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);
CREATE INDEX idx_comments_parent_id ON comments(parent_id);
CREATE INDEX idx_comments_status ON comments(status);
CREATE INDEX idx_comments_created_at ON comments(created_at);
CREATE INDEX idx_comments_post_status ON comments(post_id, status);

CREATE INDEX idx_tags_slug ON tags(slug);
CREATE INDEX idx_tags_parent_id ON tags(parent_id);
CREATE INDEX idx_tags_sort_order ON tags(sort_order);
CREATE INDEX idx_tags_is_active ON tags(is_active);

CREATE INDEX idx_post_tags_post_id ON post_tags(post_id);
CREATE INDEX idx_post_tags_tag_id ON post_tags(tag_id);

CREATE INDEX idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX idx_user_sessions_token_hash ON user_sessions(token_hash);
CREATE INDEX idx_user_sessions_expires_at ON user_sessions(expires_at);
CREATE INDEX idx_user_sessions_is_active ON user_sessions(is_active);

CREATE INDEX idx_post_views_post_id ON post_views(post_id);
CREATE INDEX idx_post_views_user_id ON post_views(user_id);
CREATE INDEX idx_post_views_viewed_at ON post_views(viewed_at);

CREATE INDEX idx_notifications_user_id ON notifications(user_id);
CREATE INDEX idx_notifications_type ON notifications(type);
CREATE INDEX idx_notifications_read_at ON notifications(read_at);
CREATE INDEX idx_notifications_created_at ON notifications(created_at DESC);

-- Create triggers for updated_at timestamps
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_posts_updated_at
    BEFORE UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_comments_updated_at
    BEFORE UPDATE ON comments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Function to automatically set published_at when status changes to published
CREATE OR REPLACE FUNCTION set_published_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Set published_at when status changes to 'published'
    IF NEW.status = 'published' AND OLD.status != 'published' THEN
        NEW.published_at = NOW();
    -- Clear published_at when status changes from 'published'
    ELSIF NEW.status != 'published' AND OLD.status = 'published' THEN
        NEW.published_at = NULL;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_post_published_at
    BEFORE UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION set_published_at();

-- Function to generate unique slugs
CREATE OR REPLACE FUNCTION generate_unique_slug(base_slug TEXT, table_name TEXT)
RETURNS TEXT AS $$
DECLARE
    counter INTEGER := 0;
    new_slug TEXT := base_slug;
    exists_count INTEGER;
BEGIN
    LOOP
        -- Check if slug exists
        EXECUTE format('SELECT COUNT(*) FROM %I WHERE slug = $1', table_name)
        USING new_slug INTO exists_count;

        -- If slug doesn't exist, return it
        IF exists_count = 0 THEN
            RETURN new_slug;
        END IF;

        -- Increment counter and try again
        counter := counter + 1;
        new_slug := base_slug || '-' || counter;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Function to create slug from title
CREATE OR REPLACE FUNCTION slugify(text_input TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN lower(
        regexp_replace(
            regexp_replace(
                unaccent(text_input),
                '[^a-zA-Z0-9\s-]', '', 'g'
            ),
            '\s+', '-', 'g'
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-generate slug for posts
CREATE OR REPLACE FUNCTION generate_post_slug()
RETURNS TRIGGER AS $$
BEGIN
    -- Only generate slug if not provided or empty
    IF NEW.slug IS NULL OR NEW.slug = '' THEN
        NEW.slug = generate_unique_slug(slugify(NEW.title), 'posts');
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER generate_post_slug_trigger
    BEFORE INSERT OR UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION generate_post_slug();

-- Trigger to auto-generate slug for tags
CREATE OR REPLACE FUNCTION generate_tag_slug()
RETURNS TRIGGER AS $$
BEGIN
    -- Only generate slug if not provided or empty
    IF NEW.slug IS NULL OR NEW.slug = '' THEN
        NEW.slug = generate_unique_slug(slugify(NEW.name), 'tags');
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER generate_tag_slug_trigger
    BEFORE INSERT OR UPDATE ON tags
    FOR EACH ROW EXECUTE FUNCTION generate_tag_slug();

-- Create views for common queries (FraiseQL will use these for optimized queries)
CREATE VIEW v_published_posts AS
SELECT p.*, u.username as author_username, u.email as author_email
FROM posts p
JOIN users u ON p.author_id = u.id
WHERE p.status = 'published' AND p.published_at <= NOW();

CREATE VIEW v_post_with_stats AS
SELECT
    p.*,
    u.username as author_username,
    COALESCE(c.comment_count, 0) as comment_count,
    COALESCE(v.view_count, 0) as view_count,
    COALESCE(t.tag_count, 0) as tag_count
FROM posts p
JOIN users u ON p.author_id = u.id
LEFT JOIN (
    SELECT post_id, COUNT(*) as comment_count
    FROM comments
    WHERE status = 'approved'
    GROUP BY post_id
) c ON p.id = c.post_id
LEFT JOIN (
    SELECT post_id, COUNT(*) as view_count
    FROM post_views
    GROUP BY post_id
) v ON p.id = v.post_id
LEFT JOIN (
    SELECT post_id, COUNT(*) as tag_count
    FROM post_tags
    GROUP BY post_id
) t ON p.id = t.post_id;

-- Create materialized view for popular content (refresh periodically)
CREATE MATERIALIZED VIEW mv_popular_posts AS
SELECT
    p.id,
    p.title,
    p.slug,
    p.author_id,
    u.username as author_username,
    p.published_at,
    COUNT(DISTINCT pv.id) as view_count,
    COUNT(DISTINCT c.id) as comment_count,
    (
        COUNT(DISTINCT pv.id) * 1.0 +
        COUNT(DISTINCT c.id) * 2.0 +
        EXTRACT(EPOCH FROM (NOW() - p.published_at)) / 86400 * -0.1
    ) as popularity_score
FROM posts p
JOIN users u ON p.author_id = u.id
LEFT JOIN post_views pv ON p.id = pv.post_id
LEFT JOIN comments c ON p.id = c.post_id AND c.status = 'approved'
WHERE p.status = 'published'
GROUP BY p.id, u.username
ORDER BY popularity_score DESC;

-- Create index on materialized view
CREATE UNIQUE INDEX idx_mv_popular_posts_id ON mv_popular_posts(id);
CREATE INDEX idx_mv_popular_posts_score ON mv_popular_posts(popularity_score DESC);

-- Function to refresh popular posts materialized view
CREATE OR REPLACE FUNCTION refresh_popular_posts()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_popular_posts;
END;
$$ LANGUAGE plpgsql;

-- Create scheduled job to refresh popular posts (if pg_cron is available)
-- SELECT cron.schedule('refresh-popular-posts', '*/15 * * * *', 'SELECT refresh_popular_posts();');

-- Grant permissions (adjust as needed for your application)
-- These would typically be more restrictive in production
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO PUBLIC;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO PUBLIC;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO PUBLIC;
