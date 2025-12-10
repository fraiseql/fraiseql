-- Migration 001: Initial CQRS Blog Schema with Trinity Pattern
-- This demonstrates the FraiseQL CQRS pattern:
-- - Command tables (tb_*): Normalized write models with Trinity identifiers
-- - Query tables (tv_*): Denormalized JSONB read models

-- ============================================================================
-- COMMAND SIDE: Normalized tables for writes (tb_* prefix) with Trinity Pattern
-- ============================================================================

-- Users table (command side) - Trinity Pattern
CREATE TABLE tb_user (
    -- Trinity Identifiers
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,     -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                       -- Human-readable (username)

    -- User data
    email TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL UNIQUE,
    full_name TEXT NOT NULL,
    bio TEXT,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_username ON tb_user(username);
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier);

-- Posts table (command side) - Trinity Pattern with INT foreign keys
CREATE TABLE tb_post (
    -- Trinity Identifiers
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,     -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                       -- Human-readable (slug)

    -- Post data
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,  -- Fast INT FK!
    published BOOLEAN NOT NULL DEFAULT FALSE,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tb_post_id ON tb_post(id);
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_fk_author ON tb_post(fk_author);  -- Fast INT FK index
CREATE INDEX idx_tb_post_published ON tb_post(published);
CREATE INDEX idx_tb_post_created ON tb_post(created_at DESC);

-- Comments table (command side) - Trinity Pattern with INT foreign keys
CREATE TABLE tb_comment (
    -- Trinity Identifiers
    pk_comment INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,        -- Public API (secure UUID)
    identifier TEXT UNIQUE,                                   -- Optional for comments

    -- Comment data
    fk_post INT NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,    -- Fast INT FK!
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,  -- Fast INT FK!
    content TEXT NOT NULL,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tb_comment_id ON tb_comment(id);
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);    -- Fast INT FK index
CREATE INDEX idx_tb_comment_fk_author ON tb_comment(fk_author);  -- Fast INT FK index
CREATE INDEX idx_tb_comment_created ON tb_comment(created_at DESC);

-- ============================================================================
-- QUERY SIDE: Denormalized JSONB tables for reads (tv_* prefix)
-- ============================================================================

-- Users view (query side) - denormalized with post count
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,  -- UUID for GraphQL API
    identifier TEXT UNIQUE NOT NULL,  -- Human-readable identifier
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Posts view (query side) - denormalized with author and comments
CREATE TABLE tv_post (
    id UUID PRIMARY KEY,  -- UUID for GraphQL API
    identifier TEXT UNIQUE NOT NULL,  -- Human-readable slug
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Comments view (query side) - denormalized with author info
CREATE TABLE tv_comment (
    id UUID PRIMARY KEY,  -- UUID for GraphQL API
    identifier TEXT UNIQUE,  -- Optional for comments
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- GIN indexes for fast JSONB queries
CREATE INDEX idx_tv_user_data ON tv_user USING GIN(data);
CREATE INDEX idx_tv_user_identifier ON tv_user(identifier);

CREATE INDEX idx_tv_post_data ON tv_post USING GIN(data);
CREATE INDEX idx_tv_post_identifier ON tv_post(identifier);

CREATE INDEX idx_tv_comment_data ON tv_comment USING GIN(data);

-- ============================================================================
-- SYNC FUNCTIONS: Explicit sync from command (tb_*) to query (tv_*) side
-- ============================================================================

-- Sync function for user: tb_user → tv_user
CREATE OR REPLACE FUNCTION fn_sync_tv_user(p_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user (id, identifier, data, updated_at)
    SELECT
        u.id,
        u.identifier,
        jsonb_build_object(
            'id', u.id::text,
            'identifier', u.identifier,
            'email', u.email,
            'username', u.username,
            'fullName', u.full_name,  -- camelCase for GraphQL
            'bio', u.bio,
            'createdAt', u.created_at,  -- camelCase for GraphQL
            'updatedAt', u.updated_at,  -- camelCase for GraphQL
            'postCount', COALESCE(
                (SELECT COUNT(*) FROM tb_post WHERE fk_author = u.pk_user),
                0
            )
        ),
        NOW()
    FROM tb_user u
    WHERE u.id = p_id
    ON CONFLICT (id) DO UPDATE
    SET
        identifier = EXCLUDED.identifier,
        data = EXCLUDED.data,
        updated_at = EXCLUDED.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Sync function for post: tb_post → tv_post
CREATE OR REPLACE FUNCTION fn_sync_tv_post(p_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_post (id, identifier, data, updated_at)
    SELECT
        p.id,
        p.identifier,
        jsonb_build_object(
            'id', p.id::text,
            'identifier', p.identifier,
            'title', p.title,
            'content', p.content,
            'published', p.published,
            'createdAt', p.created_at,  -- camelCase for GraphQL
            'updatedAt', p.updated_at,  -- camelCase for GraphQL
            'author', jsonb_build_object(
                'id', u.id::text,
                'identifier', u.identifier,
                'username', u.username,
                'fullName', u.full_name
            ),
            'commentCount', COALESCE(
                (SELECT COUNT(*) FROM tb_comment WHERE fk_post = p.pk_post),
                0
            )
        ),
        NOW()
    FROM tb_post p
    JOIN tb_user u ON u.pk_user = p.fk_author  -- Fast INT join!
    WHERE p.id = p_id
    ON CONFLICT (id) DO UPDATE
    SET
        identifier = EXCLUDED.identifier,
        data = EXCLUDED.data,
        updated_at = EXCLUDED.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Sync function for comment: tb_comment → tv_comment
CREATE OR REPLACE FUNCTION fn_sync_tv_comment(p_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_comment (id, identifier, data, updated_at)
    SELECT
        c.id,
        c.identifier,
        jsonb_build_object(
            'id', c.id::text,
            'identifier', c.identifier,
            'content', c.content,
            'createdAt', c.created_at,  -- camelCase for GraphQL
            'updatedAt', c.updated_at,  -- camelCase for GraphQL
            'author', jsonb_build_object(
                'id', u.id::text,
                'identifier', u.identifier,
                'username', u.username,
                'fullName', u.full_name
            ),
            'post', jsonb_build_object(
                'id', p.id::text,
                'identifier', p.identifier,
                'title', p.title
            )
        ),
        NOW()
    FROM tb_comment c
    JOIN tb_user u ON u.pk_user = c.fk_author  -- Fast INT join!
    JOIN tb_post p ON p.pk_post = c.fk_post    -- Fast INT join!
    WHERE c.id = p_id
    ON CONFLICT (id) DO UPDATE
    SET
        identifier = EXCLUDED.identifier,
        data = EXCLUDED.data,
        updated_at = EXCLUDED.updated_at;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- SYNC TRACKING: Track sync operations for monitoring
-- ============================================================================

CREATE TABLE sync_log (
    id BIGSERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    operation TEXT NOT NULL, -- 'incremental', 'full', 'batch'
    duration_ms INTEGER NOT NULL,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sync_log_entity ON sync_log(entity_type, created_at DESC);
CREATE INDEX idx_sync_log_created ON sync_log(created_at DESC);

-- ============================================================================
-- FUNCTIONS: Helper functions for the application
-- ============================================================================

-- Update updated_at timestamp automatically
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply to command tables
CREATE TRIGGER update_tb_user_updated_at BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_post_updated_at BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_comment_updated_at BEFORE UPDATE ON tb_comment
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- SEED DATA: Sample data for testing
-- ============================================================================

-- Insert sample users
INSERT INTO tb_user (id, identifier, email, username, full_name, bio) VALUES
    ('00000000-0000-0000-0000-000000000001', 'alice', 'alice@example.com', 'alice', 'Alice Johnson', 'Tech enthusiast and blogger'),
    ('00000000-0000-0000-0000-000000000002', 'bob', 'bob@example.com', 'bob', 'Bob Smith', 'Software engineer'),
    ('00000000-0000-0000-0000-000000000003', 'charlie', 'charlie@example.com', 'charlie', 'Charlie Brown', 'Writer and photographer');

-- Insert sample posts
INSERT INTO tb_post (id, identifier, title, content, fk_author, published) VALUES
    ('00000000-0000-0000-0001-000000000001',
     'getting-started-fraiseql',
     'Getting Started with FraiseQL',
     'FraiseQL is a revolutionary GraphQL framework that solves the N+1 query problem using CQRS and explicit sync patterns.',
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000001'),
     true),
    ('00000000-0000-0000-0001-000000000002',
     'why-cqrs-matters',
     'Why CQRS Matters',
     'Command Query Responsibility Segregation separates read and write operations for better performance and scalability.',
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000001'),
     true),
    ('00000000-0000-0000-0001-000000000003',
     'explicit-sync-vs-triggers',
     'Explicit Sync vs Triggers',
     'FraiseQL uses explicit sync calls instead of database triggers for better visibility and control.',
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000002'),
     true);

-- Insert sample comments
INSERT INTO tb_comment (fk_post, fk_author, content) VALUES
    ((SELECT pk_post FROM tb_post WHERE id = '00000000-0000-0000-0001-000000000001'),
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000002'),
     'Great introduction! Looking forward to trying it out.'),
    ((SELECT pk_post FROM tb_post WHERE id = '00000000-0000-0000-0001-000000000001'),
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000003'),
     'This looks very promising for my project.'),
    ((SELECT pk_post FROM tb_post WHERE id = '00000000-0000-0000-0001-000000000002'),
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000003'),
     'CQRS has been a game-changer for our team.'),
    ((SELECT pk_post FROM tb_post WHERE id = '00000000-0000-0000-0001-000000000003'),
     (SELECT pk_user FROM tb_user WHERE id = '00000000-0000-0000-0000-000000000001'),
     'I agree, explicit is better than implicit!');

-- Sync all seed data to query side (tv_* tables)
SELECT fn_sync_tv_user('00000000-0000-0000-0000-000000000001');
SELECT fn_sync_tv_user('00000000-0000-0000-0000-000000000002');
SELECT fn_sync_tv_user('00000000-0000-0000-0000-000000000003');

SELECT fn_sync_tv_post('00000000-0000-0000-0001-000000000001');
SELECT fn_sync_tv_post('00000000-0000-0000-0001-000000000002');
SELECT fn_sync_tv_post('00000000-0000-0000-0001-000000000003');

SELECT fn_sync_tv_comment(id) FROM tb_comment;

-- ============================================================================
-- COMMENTS: Documentation for Trinity Pattern and CQRS
-- ============================================================================

COMMENT ON TABLE tb_user IS 'Command side: Users table with Trinity pattern (pk_user INT, id UUID, identifier TEXT)';
COMMENT ON TABLE tb_post IS 'Command side: Posts table with Trinity pattern (pk_post INT, id UUID, identifier TEXT)';
COMMENT ON TABLE tb_comment IS 'Command side: Comments table with Trinity pattern (pk_comment INT, id UUID, identifier TEXT)';

COMMENT ON TABLE tv_user IS 'Query side: Denormalized users with JSONB data and pre-computed aggregations';
COMMENT ON TABLE tv_post IS 'Query side: Denormalized posts with embedded author and comment count';
COMMENT ON TABLE tv_comment IS 'Query side: Denormalized comments with embedded author and post info';

COMMENT ON FUNCTION fn_sync_tv_user IS 'Explicit sync: tb_user → tv_user (call after INSERT/UPDATE/DELETE on tb_user)';
COMMENT ON FUNCTION fn_sync_tv_post IS 'Explicit sync: tb_post → tv_post (call after INSERT/UPDATE/DELETE on tb_post)';
COMMENT ON FUNCTION fn_sync_tv_comment IS 'Explicit sync: tb_comment → tv_comment (call after INSERT/UPDATE/DELETE on tb_comment)';

COMMENT ON COLUMN tb_post.fk_author IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_post IS 'Foreign key to tb_post.pk_post (INT) - 10x faster than UUID joins';
COMMENT ON COLUMN tb_comment.fk_author IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';
