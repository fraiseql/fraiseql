-- FraiseQL Basic Example - Database Setup
-- PostgreSQL

-- Drop existing objects if present
DROP VIEW IF EXISTS v_posts CASCADE;
DROP VIEW IF EXISTS v_users CASCADE;
DROP TABLE IF EXISTS posts CASCADE;
DROP TABLE IF EXISTS users CASCADE;

-- Create users table
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create posts table
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    author_id INTEGER NOT NULL REFERENCES users(id),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_users_email ON users(email);

-- Create views that return JSONB (FraiseQL pattern)
-- Each view returns rows with a 'data' column containing the JSONB representation

CREATE VIEW v_users AS
SELECT
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM users;

CREATE VIEW v_posts AS
SELECT
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author_id', p.author_id,
        'author', jsonb_build_object(
            'id', u.id,
            'name', u.name,
            'email', u.email,
            'created_at', u.created_at
        ),
        'created_at', p.created_at
    ) AS data
FROM posts p
JOIN users u ON p.author_id = u.id;

-- Insert sample data
INSERT INTO users (name, email) VALUES
    ('Alice Johnson', 'alice@example.com'),
    ('Bob Smith', 'bob@example.com'),
    ('Charlie Brown', 'charlie@example.com');

INSERT INTO posts (title, content, author_id) VALUES
    ('Getting Started with FraiseQL', 'FraiseQL is a compiled GraphQL execution engine...', 1),
    ('Database Views for GraphQL', 'Learn how to use database views with FraiseQL...', 1),
    ('Performance Tips', 'Here are some tips for optimizing your FraiseQL queries...', 2),
    ('Hello World', 'My first blog post using FraiseQL!', 3);

-- Verify data
SELECT 'Users:' AS info;
SELECT * FROM v_users;

SELECT 'Posts:' AS info;
SELECT * FROM v_posts;
