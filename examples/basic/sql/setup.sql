-- FraiseQL Basic Example - Database Setup (Trinity Pattern)
-- PostgreSQL
-- Pattern: tb_* (table), pk_* (INTEGER primary key), fk_* (INTEGER foreign key), id (UUID), v_* (view)

-- Drop existing objects if present
DROP TABLE IF EXISTS tb_post CASCADE;
DROP TABLE IF EXISTS tb_user CASCADE;

-- Create user table (Trinity Pattern)
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create post table (Trinity Pattern)
CREATE TABLE tb_post (
    pk_post SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    fk_user INTEGER NOT NULL REFERENCES tb_user(pk_user),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes
CREATE INDEX idx_tb_post_fk_user ON tb_post(fk_user);
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_post_id ON tb_post(id);

-- Create views (Trinity Pattern v_* naming)
-- Each view returns pk_* (internal joins), id (native UUID for id lookups)
-- and data (the JSONB payload the runtime reads)
CREATE VIEW v_user AS
SELECT
    pk_user,
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user;

CREATE VIEW v_post AS
SELECT
    p.pk_post,
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author_id', u.id,
        'author_name', u.name,
        'author_email', u.email,
        'created_at', p.created_at
    ) AS data
FROM tb_post p
JOIN tb_user u ON p.fk_user = u.pk_user;

-- Insert sample data
-- Fixed UUIDs, so the ids in this example's README and queries/*.vars.json are
-- stable across a rebuild. A real application leaves `id` to its DEFAULT.
INSERT INTO tb_user (id, name, email) VALUES
    ('11111111-0000-4000-8000-000000000001', 'Alice Johnson', 'alice@example.com'),
    ('11111111-0000-4000-8000-000000000002', 'Bob Smith', 'bob@example.com'),
    ('11111111-0000-4000-8000-000000000003', 'Charlie Brown', 'charlie@example.com');

INSERT INTO tb_post (id, title, content, fk_user) VALUES
    ('22222222-0000-4000-8000-000000000001', 'Getting Started with FraiseQL', 'FraiseQL is a compiled GraphQL execution engine...', 1),
    ('22222222-0000-4000-8000-000000000002', 'Database Views for GraphQL', 'Learn how to use database views with FraiseQL...', 1),
    ('22222222-0000-4000-8000-000000000003', 'Performance Tips', 'Here are some tips for optimizing your FraiseQL queries...', 2),
    ('22222222-0000-4000-8000-000000000004', 'Hello World', 'My first blog post using FraiseQL!', 3);

-- Verify data
SELECT 'Users:' AS info;
SELECT * FROM v_user;

SELECT 'Posts:' AS info;
SELECT * FROM v_post;
