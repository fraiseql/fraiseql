-- Migration 001: Initial CQRS Blog Schema
-- This demonstrates the FraiseQL CQRS pattern:
-- - Command tables (tb_*): Normalized write models
-- - Query tables (tv_*): Denormalized JSONB read models

-- ============================================================================
-- COMMAND SIDE: Normalized tables for writes (tb_* prefix)
-- ============================================================================

-- Users table (command side)
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL UNIQUE,
    full_name TEXT NOT NULL,
    bio TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_username ON tb_user(username);

-- Posts table (command side)
CREATE TABLE tb_post (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    author_id UUID NOT NULL REFERENCES tb_user(id) ON DELETE CASCADE,
    published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tb_post_author ON tb_post(author_id);
CREATE INDEX idx_tb_post_published ON tb_post(published);
CREATE INDEX idx_tb_post_created ON tb_post(created_at DESC);

-- Comments table (command side)
CREATE TABLE tb_comment (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES tb_user(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tb_comment_post ON tb_comment(post_id);
CREATE INDEX idx_tb_comment_author ON tb_comment(author_id);
CREATE INDEX idx_tb_comment_created ON tb_comment(created_at DESC);

-- ============================================================================
-- QUERY SIDE: Denormalized JSONB tables for reads (tv_* prefix)
-- ============================================================================

-- Users view (query side) - denormalized with post count
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Posts view (query side) - denormalized with author and comments
CREATE TABLE tv_post (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Comments view (query side) - denormalized with author info
CREATE TABLE tv_comment (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- GIN indexes for fast JSONB queries
CREATE INDEX idx_tv_user_data ON tv_user USING GIN(data);
CREATE INDEX idx_tv_post_data ON tv_post USING GIN(data);
CREATE INDEX idx_tv_comment_data ON tv_comment USING GIN(data);

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
INSERT INTO tb_user (id, email, username, full_name, bio) VALUES
    ('00000000-0000-0000-0000-000000000001', 'alice@example.com', 'alice', 'Alice Johnson', 'Tech enthusiast and blogger'),
    ('00000000-0000-0000-0000-000000000002', 'bob@example.com', 'bob', 'Bob Smith', 'Software engineer'),
    ('00000000-0000-0000-0000-000000000003', 'charlie@example.com', 'charlie', 'Charlie Brown', 'Writer and photographer');

-- Insert sample posts
INSERT INTO tb_post (id, title, content, author_id, published) VALUES
    ('00000000-0000-0000-0001-000000000001',
     'Getting Started with FraiseQL',
     'FraiseQL is a revolutionary GraphQL framework that solves the N+1 query problem using CQRS and explicit sync patterns.',
     '00000000-0000-0000-0000-000000000001',
     true),
    ('00000000-0000-0000-0001-000000000002',
     'Why CQRS Matters',
     'Command Query Responsibility Segregation separates read and write operations for better performance and scalability.',
     '00000000-0000-0000-0000-000000000001',
     true),
    ('00000000-0000-0000-0001-000000000003',
     'Explicit Sync vs Triggers',
     'FraiseQL uses explicit sync calls instead of database triggers for better visibility and control.',
     '00000000-0000-0000-0000-000000000002',
     true);

-- Insert sample comments
INSERT INTO tb_comment (post_id, author_id, content) VALUES
    ('00000000-0000-0000-0001-000000000001', '00000000-0000-0000-0000-000000000002', 'Great introduction! Looking forward to trying it out.'),
    ('00000000-0000-0000-0001-000000000001', '00000000-0000-0000-0000-000000000003', 'This looks very promising for my project.'),
    ('00000000-0000-0000-0001-000000000002', '00000000-0000-0000-0000-000000000003', 'CQRS has been a game-changer for our team.'),
    ('00000000-0000-0000-0001-000000000003', '00000000-0000-0000-0000-000000000001', 'I agree, explicit is better than implicit!');
