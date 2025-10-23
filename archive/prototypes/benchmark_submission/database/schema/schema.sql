-- GraphQL Benchmark Database Schema - CQRS Architecture
-- FraiseQL implementation with tb_/tv_ table separation
-- tb_ tables: normalized write-side (source of truth)
-- tv_ tables: denormalized read-side (optimized for queries)

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Create schema
CREATE SCHEMA IF NOT EXISTS benchmark;
SET search_path TO benchmark, public;

-- ==============================================================================
-- WRITE SIDE (Command): All tb_* tables
-- ==============================================================================

-- User table (command side - normalized)
CREATE TABLE tb_user (
    -- Trinity pattern identifiers
    pk_user SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,

    -- User data
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    age INTEGER,
    city VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Post table (command side - normalized)
CREATE TABLE tb_post (
    -- Trinity pattern identifiers
    pk_post SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,

    -- Post data
    title VARCHAR(255) NOT NULL,
    content TEXT,
    published BOOLEAN DEFAULT FALSE,
    fk_author INTEGER NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Comment table (command side - normalized)
CREATE TABLE tb_comment (
    -- Trinity pattern identifiers
    pk_comment SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,

    -- Comment data
    content TEXT NOT NULL,
    fk_post INTEGER NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    fk_author INTEGER NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- ==============================================================================
-- READ SIDE (Query): All v_* views and tv_* tables
-- ==============================================================================

-- User view (read side - denormalized)
CREATE VIEW v_user AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id::text,
        'identifier', u.identifier,
        'name', u.name,
        'email', u.email,
        'age', u.age,
        'city', u.city,
        'createdAt', u.created_at
    ) AS data
FROM tb_user u;

-- Post view with author (read side - denormalized)
CREATE VIEW v_post AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id::text,
        'identifier', p.identifier,
        'title', p.title,
        'content', p.content,
        'published', p.published,
        'authorId', u.id::text,
        'createdAt', p.created_at
    ) AS data
FROM tb_post p
JOIN tb_user u ON u.pk_user = p.fk_author;

-- Comment view with author and post (read side - denormalized)
CREATE VIEW v_comment AS
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id::text,
        'content', c.content,
        'postId', p.id::text,
        'authorId', u.id::text,
        'createdAt', c.created_at
    ) AS data
FROM tb_comment c
JOIN tb_user u ON u.pk_user = c.fk_author
JOIN tb_post p ON p.pk_post = c.fk_post;

-- Denormalized tables for optimal read performance
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB
);

CREATE TABLE tv_post (
    id UUID PRIMARY KEY,
    data JSONB
);

CREATE TABLE tv_comment (
    id UUID PRIMARY KEY,
    data JSONB
);

-- ==============================================================================
-- SYNC FUNCTIONS: Populate tv_* tables from v_* views
-- ==============================================================================

-- Sync tv_user table from v_user view
CREATE OR REPLACE FUNCTION sync_tv_user() RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_user;
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user;
END;
$$ LANGUAGE plpgsql;

-- Sync tv_post table from v_post view
CREATE OR REPLACE FUNCTION sync_tv_post() RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_post;
    INSERT INTO tv_post (id, data)
    SELECT id, data FROM v_post;
END;
$$ LANGUAGE plpgsql;

-- Sync tv_comment table from v_comment view
CREATE OR REPLACE FUNCTION sync_tv_comment() RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_comment;
    INSERT INTO tv_comment (id, data)
    SELECT id, data FROM v_comment;
END;
$$ LANGUAGE plpgsql;

-- Sync all denormalized tables
CREATE OR REPLACE FUNCTION sync_all_tv_tables() RETURNS VOID AS $$
BEGIN
    PERFORM sync_tv_user();
    PERFORM sync_tv_post();
    PERFORM sync_tv_comment();
END;
$$ LANGUAGE plpgsql;

-- ==============================================================================
-- TRIGGERS: Auto-sync tv_* tables on tb_* changes
-- ==============================================================================

-- Update timestamps trigger
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply timestamp triggers
CREATE TRIGGER update_tb_user_updated_at
    BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_post_updated_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_comment_updated_at
    BEFORE UPDATE ON tb_comment
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Auto-sync triggers for denormalized tables
CREATE OR REPLACE FUNCTION sync_user_on_change()
RETURNS TRIGGER AS $$
BEGIN
    -- Sync single user
    DELETE FROM tv_user WHERE id = COALESCE(NEW.id, OLD.id);
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user WHERE id = COALESCE(NEW.id, OLD.id);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sync_post_on_change()
RETURNS TRIGGER AS $$
BEGIN
    -- Sync single post
    DELETE FROM tv_post WHERE id = COALESCE(NEW.id, OLD.id);
    INSERT INTO tv_post (id, data)
    SELECT id, data FROM v_post WHERE id = COALESCE(NEW.id, OLD.id);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sync_comment_on_change()
RETURNS TRIGGER AS $$
BEGIN
    -- Sync single comment
    DELETE FROM tv_comment WHERE id = COALESCE(NEW.id, OLD.id);
    INSERT INTO tv_comment (id, data)
    SELECT id, data FROM v_comment WHERE id = COALESCE(NEW.id, OLD.id);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Apply sync triggers
CREATE TRIGGER sync_tv_user_on_change
    AFTER INSERT OR UPDATE OR DELETE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION sync_user_on_change();

CREATE TRIGGER sync_tv_post_on_change
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION sync_post_on_change();

CREATE TRIGGER sync_tv_comment_on_change
    AFTER INSERT OR UPDATE OR DELETE ON tb_comment
    FOR EACH ROW EXECUTE FUNCTION sync_comment_on_change();

-- ==============================================================================
-- INDEXES: Optimize query performance
-- ==============================================================================

-- Primary key indexes (already created with PRIMARY KEY)

-- Foreign key indexes
CREATE INDEX idx_tb_post_fk_author ON tb_post(fk_author);
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);
CREATE INDEX idx_tb_comment_fk_author ON tb_comment(fk_author);

-- Query optimization indexes
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_name ON tb_user(name);
CREATE INDEX idx_tb_post_published ON tb_post(published);
CREATE INDEX idx_tb_post_created_at ON tb_post(created_at);
CREATE INDEX idx_tb_comment_created_at ON tb_comment(created_at);

-- JSONB indexes for denormalized tables
CREATE INDEX idx_tv_user_data_name ON tv_user USING gin((data->'name'));
CREATE INDEX idx_tv_user_data_email ON tv_user USING gin((data->'email'));
CREATE INDEX idx_tv_post_data_title ON tv_post USING gin((data->'title'));
CREATE INDEX idx_tv_post_data_published ON tv_post((data->'published'));
CREATE INDEX idx_tv_comment_data_post_id ON tv_comment((data->'postId'));

-- ==============================================================================
-- PERMISSIONS: Grant access to benchmark user
-- ==============================================================================

GRANT USAGE ON SCHEMA benchmark TO benchmark;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA benchmark TO benchmark;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA benchmark TO benchmark;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA benchmark TO benchmark;
