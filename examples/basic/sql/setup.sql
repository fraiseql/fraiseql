-- FraiseQL Basic Example - Database Setup (Trinity Pattern)
-- PostgreSQL
-- Pattern: tb_* (table), pk_* (INTEGER primary key), fk_* (INTEGER foreign key), id (UUID), v_* (view)

-- Drop existing objects if present
DROP TABLE IF EXISTS tb_posts CASCADE;
DROP TABLE IF EXISTS tb_users CASCADE;

-- Create users table (Trinity Pattern)
CREATE TABLE tb_users (
    pk_user SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create posts table (Trinity Pattern)
CREATE TABLE tb_posts (
    pk_post SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    fk_user INTEGER NOT NULL REFERENCES tb_users(pk_user),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes
CREATE INDEX idx_tb_posts_fk_user ON tb_posts(fk_user);
CREATE INDEX idx_tb_users_email ON tb_users(email);
CREATE INDEX idx_tb_users_id ON tb_users(id);
CREATE INDEX idx_tb_posts_id ON tb_posts(id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_users AS
SELECT
    pk_user,
    id,
    name,
    email,
    created_at
FROM tb_users;

CREATE VIEW v_posts AS
SELECT
    p.pk_post,
    p.id,
    p.title,
    p.content,
    p.fk_user,
    u.id AS author_id,
    u.name AS author_name,
    u.email AS author_email,
    p.created_at
FROM tb_posts p
JOIN tb_users u ON p.fk_user = u.pk_user;

-- Insert sample data
INSERT INTO tb_users (name, email) VALUES
    ('Alice Johnson', 'alice@example.com'),
    ('Bob Smith', 'bob@example.com'),
    ('Charlie Brown', 'charlie@example.com');

INSERT INTO tb_posts (title, content, fk_user) VALUES
    ('Getting Started with FraiseQL', 'FraiseQL is a compiled GraphQL execution engine...', 1),
    ('Database Views for GraphQL', 'Learn how to use database views with FraiseQL...', 1),
    ('Performance Tips', 'Here are some tips for optimizing your FraiseQL queries...', 2),
    ('Hello World', 'My first blog post using FraiseQL!', 3);

-- Verify data
SELECT 'Users:' AS info;
SELECT * FROM v_users;

SELECT 'Posts:' AS info;
SELECT * FROM v_posts;
