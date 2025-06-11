-- Blog API CQRS Schema - Write Side Tables
-- All write-side tables are prefixed with tb_

-- Create extension for UUID generation if not exists
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Users table (write side)
CREATE TABLE IF NOT EXISTS tb_users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Posts table (write side)
CREATE TABLE IF NOT EXISTS tb_posts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    author_id UUID NOT NULL REFERENCES tb_users(id) ON DELETE CASCADE,
    title VARCHAR(500) NOT NULL,
    slug VARCHAR(500) NOT NULL UNIQUE,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[] DEFAULT ARRAY[]::TEXT[],
    is_published BOOLEAN DEFAULT false,
    published_at TIMESTAMPTZ,
    view_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Comments table (write side)
CREATE TABLE IF NOT EXISTS tb_comments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL REFERENCES tb_posts(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES tb_users(id) ON DELETE CASCADE,
    parent_id UUID REFERENCES tb_comments(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    is_edited BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_tb_users_email ON tb_users(email);
CREATE INDEX idx_tb_users_active ON tb_users(is_active);

CREATE INDEX idx_tb_posts_author ON tb_posts(author_id);
CREATE INDEX idx_tb_posts_slug ON tb_posts(slug);
CREATE INDEX idx_tb_posts_published ON tb_posts(is_published);
CREATE INDEX idx_tb_posts_created ON tb_posts(created_at);
CREATE INDEX idx_tb_posts_tags ON tb_posts USING gin(tags);

CREATE INDEX idx_tb_comments_post ON tb_comments(post_id);
CREATE INDEX idx_tb_comments_author ON tb_comments(author_id);
CREATE INDEX idx_tb_comments_parent ON tb_comments(parent_id);

-- Update timestamp trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply update triggers
CREATE TRIGGER update_tb_users_updated_at BEFORE UPDATE ON tb_users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_posts_updated_at BEFORE UPDATE ON tb_posts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_comments_updated_at BEFORE UPDATE ON tb_comments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
