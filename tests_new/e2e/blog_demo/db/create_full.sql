-- Complete Blog Demo Database Creation Script
-- This file combines all schema and seed files for easy database setup

-- ============================================================================
-- EXTENSIONS AND TYPES
-- ============================================================================

-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Case-insensitive text type for emails and usernames
CREATE EXTENSION IF NOT EXISTS "citext";

-- Trigram matching for search functionality
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Unaccent for slug generation and search
CREATE EXTENSION IF NOT EXISTS "unaccent";

-- Set timezone for application
SET timezone = 'UTC';

-- User role enumeration
CREATE TYPE user_role AS ENUM ('admin', 'moderator', 'author', 'user', 'guest');

-- Post status enumeration
CREATE TYPE post_status AS ENUM ('draft', 'published', 'archived', 'deleted');

-- Comment status enumeration
CREATE TYPE comment_status AS ENUM ('pending', 'approved', 'rejected', 'spam');

-- Mutation result type for standardized responses
CREATE TYPE mutation_result AS (
    success BOOLEAN,
    message TEXT,
    object_data JSONB,
    error_code TEXT,
    metadata JSONB
);

-- ============================================================================
-- COMMAND TABLES (WRITE SIDE)
-- ============================================================================

-- Users command table (tb_user)
CREATE TABLE IF NOT EXISTS tb_user (
    pk_user UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    identifier CITEXT UNIQUE NOT NULL, -- Business identifier (username)

    -- Flat normalized columns
    email CITEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    role user_role NOT NULL DEFAULT 'user',
    is_active BOOLEAN NOT NULL DEFAULT true,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    last_login_at TIMESTAMPTZ,

    -- Profile data as JSONB for flexibility
    profile JSONB DEFAULT '{}',
    preferences JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT username_length CHECK (length(identifier) >= 3 AND length(identifier) <= 30),
    CONSTRAINT email_format CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$')
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_user_identifier ON tb_user(identifier);
CREATE INDEX IF NOT EXISTS idx_tb_user_created_at ON tb_user(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_user_pk_user ON tb_user(pk_user);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_user_email ON tb_user(email);
CREATE INDEX IF NOT EXISTS idx_tb_user_role ON tb_user(role);
CREATE INDEX IF NOT EXISTS idx_tb_user_is_active ON tb_user(is_active);

-- JSONB indexes for profile data
CREATE INDEX IF NOT EXISTS idx_tb_user_profile_gin ON tb_user USING GIN (profile);
CREATE INDEX IF NOT EXISTS idx_tb_user_preferences_gin ON tb_user USING GIN (preferences);

-- Posts command table (tb_post)
CREATE TABLE IF NOT EXISTS tb_post (
    pk_post UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    identifier CITEXT UNIQUE NOT NULL, -- Business identifier (slug)
    fk_author UUID NOT NULL, -- Reference to tb_user.pk_user

    -- Flat normalized columns
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    status post_status NOT NULL DEFAULT 'draft',
    featured BOOLEAN NOT NULL DEFAULT false,
    published_at TIMESTAMPTZ,

    -- SEO and custom metadata as JSONB
    seo_metadata JSONB DEFAULT '{}',
    custom_fields JSONB DEFAULT '{}',

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT slug_format CHECK (identifier ~* '^[a-z0-9-]+$'),
    CONSTRAINT slug_length CHECK (length(identifier) >= 1 AND length(identifier) <= 200),
    CONSTRAINT title_length CHECK (length(title) >= 1 AND length(title) <= 200),
    CONSTRAINT published_at_logic CHECK (
        (status = 'published' AND published_at IS NOT NULL) OR
        (status != 'published')
    ),

    -- Foreign key
    CONSTRAINT fk_post_author FOREIGN KEY (fk_author) REFERENCES tb_user(pk_user) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX IF NOT EXISTS idx_tb_post_author ON tb_post(fk_author);
CREATE INDEX IF NOT EXISTS idx_tb_post_created_at ON tb_post(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_post_pk_post ON tb_post(pk_post);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_status ON tb_post(status);
CREATE INDEX IF NOT EXISTS idx_tb_post_featured ON tb_post(featured);
CREATE INDEX IF NOT EXISTS idx_tb_post_published_at ON tb_post(published_at DESC);
CREATE INDEX IF NOT EXISTS idx_tb_post_status_published_at ON tb_post(status, published_at DESC);

-- Full-text search indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_title_gin ON tb_post USING gin(to_tsvector('english', title));
CREATE INDEX IF NOT EXISTS idx_tb_post_content_gin ON tb_post USING gin(to_tsvector('english', content));
CREATE INDEX IF NOT EXISTS idx_tb_post_search_gin ON tb_post USING gin(
    to_tsvector('english', title || ' ' || COALESCE(content, '') || ' ' || COALESCE(excerpt, ''))
);

-- JSONB indexes for metadata
CREATE INDEX IF NOT EXISTS idx_tb_post_seo_metadata_gin ON tb_post USING GIN (seo_metadata);
CREATE INDEX IF NOT EXISTS idx_tb_post_custom_fields_gin ON tb_post USING GIN (custom_fields);

-- Comments command table (tb_comment)
CREATE TABLE IF NOT EXISTS tb_comment (
    pk_comment UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    fk_post UUID NOT NULL, -- Reference to tb_post.pk_post
    fk_author UUID NOT NULL, -- Reference to tb_user.pk_user
    fk_parent_comment UUID, -- Self-reference for threading

    -- Flat normalized columns
    content TEXT NOT NULL,
    status comment_status NOT NULL DEFAULT 'pending',

    -- Moderation metadata as JSONB
    moderation_data JSONB DEFAULT '{}',

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT content_length CHECK (length(content) >= 1 AND length(content) <= 2000),
    CONSTRAINT no_self_parent CHECK (pk_comment != fk_parent_comment),

    -- Foreign keys
    CONSTRAINT fk_comment_post FOREIGN KEY (fk_post) REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    CONSTRAINT fk_comment_author FOREIGN KEY (fk_author) REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    CONSTRAINT fk_comment_parent FOREIGN KEY (fk_parent_comment) REFERENCES tb_comment(pk_comment) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_comment_post ON tb_comment(fk_post);
CREATE INDEX IF NOT EXISTS idx_tb_comment_author ON tb_comment(fk_author);
CREATE INDEX IF NOT EXISTS idx_tb_comment_parent ON tb_comment(fk_parent_comment);
CREATE INDEX IF NOT EXISTS idx_tb_comment_created_at ON tb_comment(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_comment_pk_comment ON tb_comment(pk_comment);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_comment_status ON tb_comment(status);
CREATE INDEX IF NOT EXISTS idx_tb_comment_post_status ON tb_comment(fk_post, status);

-- JSONB indexes for moderation data
CREATE INDEX IF NOT EXISTS idx_tb_comment_moderation_data_gin ON tb_comment USING GIN (moderation_data);

-- Tags command table (tb_tag)
CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Internal ID (never exposed)
    identifier CITEXT UNIQUE NOT NULL, -- Business identifier (slug)
    fk_parent_tag UUID, -- Self-reference for hierarchy

    -- Flat normalized columns
    name CITEXT NOT NULL,
    description TEXT,
    color VARCHAR(7), -- Hex color code
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,

    -- Basic constraints
    CONSTRAINT slug_format CHECK (identifier ~* '^[a-z0-9-]+$'),
    CONSTRAINT slug_length CHECK (length(identifier) >= 1 AND length(identifier) <= 50),
    CONSTRAINT name_length CHECK (length(name) >= 1 AND length(name) <= 50),
    CONSTRAINT color_format CHECK (color IS NULL OR color ~* '^#[0-9a-f]{6}$'),
    CONSTRAINT no_self_parent CHECK (pk_tag != fk_parent_tag),

    -- Foreign key
    CONSTRAINT fk_tag_parent FOREIGN KEY (fk_parent_tag) REFERENCES tb_tag(pk_tag) ON DELETE SET NULL
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_tag_identifier ON tb_tag(identifier);
CREATE INDEX IF NOT EXISTS idx_tb_tag_parent ON tb_tag(fk_parent_tag);
CREATE INDEX IF NOT EXISTS idx_tb_tag_created_at ON tb_tag(created_at);
CREATE INDEX IF NOT EXISTS idx_tb_tag_pk_tag ON tb_tag(pk_tag);

-- Flat column indexes
CREATE INDEX IF NOT EXISTS idx_tb_tag_name ON tb_tag(name);
CREATE INDEX IF NOT EXISTS idx_tb_tag_is_active ON tb_tag(is_active);
CREATE INDEX IF NOT EXISTS idx_tb_tag_sort_order ON tb_tag(sort_order);
CREATE INDEX IF NOT EXISTS idx_tb_tag_active_sort ON tb_tag(is_active, sort_order);

-- Post-Tag junction table (tb_post_tag)
CREATE TABLE IF NOT EXISTS tb_post_tag (
    fk_post UUID NOT NULL, -- Reference to tb_post.pk_post
    fk_tag UUID NOT NULL, -- Reference to tb_tag.pk_tag

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,

    -- Primary key
    PRIMARY KEY (fk_post, fk_tag),

    -- Foreign keys
    CONSTRAINT fk_post_tag_post FOREIGN KEY (fk_post) REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    CONSTRAINT fk_post_tag_tag FOREIGN KEY (fk_tag) REFERENCES tb_tag(pk_tag) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX IF NOT EXISTS idx_tb_post_tag_post ON tb_post_tag(fk_post);
CREATE INDEX IF NOT EXISTS idx_tb_post_tag_tag ON tb_post_tag(fk_tag);
CREATE INDEX IF NOT EXISTS idx_tb_post_tag_created_at ON tb_post_tag(created_at);

-- ============================================================================
-- QUERY VIEWS (READ SIDE)
-- ============================================================================

-- User query view (v_user)
CREATE OR REPLACE VIEW v_user AS
SELECT
    pk_user AS id, -- Transform pk_user -> id for GraphQL
    identifier AS username,
    email,
    role,
    is_active,
    email_verified,
    created_at,
    updated_at,
    last_login_at,

    -- JSONB profile data
    COALESCE(profile, '{}'::jsonb) AS profile,
    COALESCE(preferences, '{}'::jsonb) AS preferences,
    COALESCE(metadata, '{}'::jsonb) AS metadata,

    -- Audit fields
    created_by,
    updated_by,
    version
FROM tb_user
WHERE is_active = true;

-- Grant permissions
GRANT SELECT ON v_user TO PUBLIC;

-- Post query view (v_post)
CREATE OR REPLACE VIEW v_post AS
SELECT
    p.pk_post AS id, -- Transform pk_post -> id for GraphQL
    p.identifier AS slug,
    p.title,
    p.content,
    p.excerpt,
    p.fk_author AS author_id, -- Keep as UUID reference
    p.status,
    p.featured,
    p.created_at,
    p.updated_at,
    p.published_at,

    -- JSONB metadata
    COALESCE(p.seo_metadata, '{}'::jsonb) AS seo_metadata,
    COALESCE(p.custom_fields, '{}'::jsonb) AS custom_fields,

    -- Audit fields
    p.created_by,
    p.updated_by,
    p.version
FROM tb_post p
WHERE p.status != 'deleted';

-- Grant permissions
GRANT SELECT ON v_post TO PUBLIC;

-- Comment query view (v_comment)
CREATE OR REPLACE VIEW v_comment AS
SELECT
    c.pk_comment AS id, -- Transform pk_comment -> id for GraphQL
    c.fk_post AS post_id, -- Keep as UUID reference
    c.fk_author AS author_id, -- Keep as UUID reference
    c.fk_parent_comment AS parent_id, -- Keep as UUID reference (nullable)
    c.content,
    c.status,
    c.created_at,
    c.updated_at,

    -- JSONB metadata
    COALESCE(c.moderation_data, '{}'::jsonb) AS moderation_data,

    -- Audit fields
    c.created_by,
    c.updated_by,
    c.version
FROM tb_comment c;

-- Grant permissions
GRANT SELECT ON v_comment TO PUBLIC;

-- Tag query view (v_tag)
CREATE OR REPLACE VIEW v_tag AS
SELECT
    t.pk_tag AS id, -- Transform pk_tag -> id for GraphQL
    t.identifier AS slug,
    t.name,
    t.description,
    t.color,
    t.fk_parent_tag AS parent_id, -- Keep as UUID reference (nullable)
    t.sort_order,
    t.is_active,
    t.created_at,

    -- Audit fields
    t.created_by,
    t.updated_by,
    t.version
FROM tb_tag t
WHERE t.is_active = true;

-- Grant permissions
GRANT SELECT ON v_tag TO PUBLIC;

-- ============================================================================
-- AUDIT TRIGGERS
-- ============================================================================

-- User update trigger
CREATE OR REPLACE FUNCTION update_tb_user_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_user_updated_at
    BEFORE UPDATE ON tb_user
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_user_updated_at();

-- Post update trigger
CREATE OR REPLACE FUNCTION update_tb_post_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_post_updated_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_post_updated_at();

-- Comment update trigger
CREATE OR REPLACE FUNCTION update_tb_comment_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_comment_updated_at
    BEFORE UPDATE ON tb_comment
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_comment_updated_at();

-- Tag update trigger
CREATE OR REPLACE FUNCTION update_tb_tag_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_tb_tag_updated_at
    BEFORE UPDATE ON tb_tag
    FOR EACH ROW
    EXECUTE FUNCTION update_tb_tag_updated_at();

-- ============================================================================
-- SEED DATA
-- ============================================================================

-- Create test users
INSERT INTO tb_user (
    pk_user,
    identifier,
    email,
    password_hash,
    role,
    is_active,
    email_verified,
    profile,
    created_at
) VALUES
(
    '11111111-1111-1111-1111-111111111111'::UUID,
    'admin',
    'admin@blog.demo',
    '$2b$12$dummy_hash_for_admin_password',
    'admin',
    true,
    true,
    jsonb_build_object(
        'firstName', 'Admin',
        'lastName', 'User',
        'bio', 'System Administrator',
        'website', 'https://blog.demo'
    ),
    '2024-01-01 00:00:00+00'
),
(
    '22222222-2222-2222-2222-222222222222'::UUID,
    'johndoe',
    'john.doe@example.com',
    '$2b$12$dummy_hash_for_john_password',
    'author',
    true,
    true,
    jsonb_build_object(
        'firstName', 'John',
        'lastName', 'Doe',
        'bio', 'Tech blogger and software developer',
        'website', 'https://johndoe.dev',
        'avatar_url', 'https://gravatar.com/avatar/johndoe'
    ),
    '2024-01-01 01:00:00+00'
),
(
    '33333333-3333-3333-3333-333333333333'::UUID,
    'janesmit',
    'jane.smith@example.com',
    '$2b$12$dummy_hash_for_jane_password',
    'author',
    true,
    true,
    jsonb_build_object(
        'firstName', 'Jane',
        'lastName', 'Smith',
        'bio', 'Frontend developer and UI/UX designer',
        'website', 'https://janesmith.design'
    ),
    '2024-01-01 02:00:00+00'
),
(
    '44444444-4444-4444-4444-444444444444'::UUID,
    'testuser',
    'test.user@example.com',
    '$2b$12$dummy_hash_for_test_password',
    'user',
    true,
    false,
    jsonb_build_object(
        'firstName', 'Test',
        'lastName', 'User',
        'bio', 'Just a regular user for testing'
    ),
    '2024-01-01 03:00:00+00'
),
(
    '55555555-5555-5555-5555-555555555555'::UUID,
    'moderator',
    'mod@blog.demo',
    '$2b$12$dummy_hash_for_mod_password',
    'moderator',
    true,
    true,
    jsonb_build_object(
        'firstName', 'Blog',
        'lastName', 'Moderator',
        'bio', 'Content moderator'
    ),
    '2024-01-01 04:00:00+00'
) ON CONFLICT (pk_user) DO NOTHING;

-- Create basic tags
INSERT INTO tb_tag (
    pk_tag,
    identifier,
    name,
    description,
    color,
    sort_order,
    is_active,
    created_at
) VALUES
(
    'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'::UUID,
    'graphql',
    'GraphQL',
    'GraphQL related posts and tutorials',
    '#E10098',
    1,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb'::UUID,
    'postgresql',
    'PostgreSQL',
    'PostgreSQL database tutorials and tips',
    '#336791',
    2,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'cccccccc-cccc-cccc-cccc-cccccccccccc'::UUID,
    'web-development',
    'Web Development',
    'General web development topics',
    '#61DAFB',
    3,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'dddddddd-dddd-dddd-dddd-dddddddddddd'::UUID,
    'python',
    'Python',
    'Python programming language',
    '#3776AB',
    4,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee'::UUID,
    'javascript',
    'JavaScript',
    'JavaScript and related frameworks',
    '#F7DF1E',
    5,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'ffffffff-ffff-ffff-ffff-ffffffffffff'::UUID,
    'tutorial',
    'Tutorial',
    'Step-by-step guides and tutorials',
    '#28A745',
    6,
    true,
    '2024-01-01 00:00:00+00'
) ON CONFLICT (pk_tag) DO NOTHING;

-- Set sequences to avoid conflicts
SELECT setval('tb_user_id_seq', 1000, true);
SELECT setval('tb_post_id_seq', 1000, true);
SELECT setval('tb_comment_id_seq', 1000, true);
SELECT setval('tb_tag_id_seq', 1000, true);

-- Success message
DO $$
BEGIN
    RAISE NOTICE '✅ Blog Demo Database setup completed successfully!';
    RAISE NOTICE '📊 Created: % users, % tags',
        (SELECT count(*) FROM tb_user),
        (SELECT count(*) FROM tb_tag);
END $$;
