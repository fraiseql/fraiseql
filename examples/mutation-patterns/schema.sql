-- ============================================================================
-- Test Schema for FraiseQL Mutation Patterns
-- ============================================================================
-- This schema provides tables for testing all mutation patterns.
-- Load this before running any examples.
-- ============================================================================

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============================================================================
-- Core Tables
-- ============================================================================

CREATE TABLE users (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    email text UNIQUE NOT NULL,
    name text NOT NULL,
    age integer,
    password_hash text,
    status text DEFAULT 'active' CHECK (status IN ('active', 'inactive', 'suspended')),
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

CREATE TABLE posts (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    title text NOT NULL,
    content text,
    status text DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

CREATE TABLE comments (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id uuid REFERENCES posts(id) ON DELETE CASCADE,
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    content text NOT NULL,
    created_at timestamptz DEFAULT now()
);

CREATE TABLE tags (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name text UNIQUE NOT NULL,
    color text DEFAULT '#666666'
);

CREATE TABLE post_tags (
    post_id uuid REFERENCES posts(id) ON DELETE CASCADE,
    tag_id uuid REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);

-- ============================================================================
-- Indexes for Performance
-- ============================================================================

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_status ON users(status);
CREATE INDEX idx_posts_user_id ON posts(user_id);
CREATE INDEX idx_posts_status ON posts(status);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_user_id ON comments(user_id);

-- ============================================================================
-- Mutation Response Type
-- ============================================================================

CREATE TYPE mutation_response AS (
    status text,
    message text,
    entity_id text,
    entity_type text,
    entity jsonb,
    updated_fields text[],
    cascade jsonb,
    metadata jsonb
);

-- ============================================================================
-- Helper Functions
-- ============================================================================

-- Include validation helpers
\i sql/helpers/mutation_validation.sql

-- ============================================================================
-- Sample Data
-- ============================================================================

INSERT INTO users (id, email, name, age) VALUES
    ('550e8400-e29b-41d4-a716-446655440000', 'john@example.com', 'John Doe', 30),
    ('550e8400-e29b-41d4-a716-446655440001', 'jane@example.com', 'Jane Smith', 25);

INSERT INTO posts (id, user_id, title, content, status) VALUES
    ('660e8400-e29b-41d4-a716-446655440000', '550e8400-e29b-41d4-a716-446655440000', 'First Post', 'Hello World!', 'published'),
    ('660e8400-e29b-41d4-a716-446655440001', '550e8400-e29b-41d4-a716-446655440001', 'Second Post', 'Another post', 'draft');

INSERT INTO tags (name, color) VALUES
    ('tutorial', '#4CAF50'),
    ('news', '#2196F3'),
    ('announcement', '#FF9800');
