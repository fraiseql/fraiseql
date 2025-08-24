-- Blog E2E Test Suite - Database Schema
-- Following PrintOptim Backend patterns for database-first architecture

-- ============================================================================
-- DOMAIN: Blog Content System
-- Purpose: Complete blog application with error testing focus
-- Pattern: Command/Query separation with materialized projections
-- ============================================================================

-- Create schemas following PrintOptim patterns
CREATE SCHEMA IF NOT EXISTS blog;      -- Command side tables (tb_*)
CREATE SCHEMA IF NOT EXISTS app;       -- API wrapper functions
CREATE SCHEMA IF NOT EXISTS core;      -- Business logic functions

-- ============================================================================
-- COMMAND SIDE - Source of Truth Tables (tb_*)
-- ============================================================================

-- Authors table - Content creators
CREATE TABLE blog.tb_author (
    id SERIAL,                                  -- Internal sequence
    pk_author UUID PRIMARY KEY DEFAULT gen_random_uuid(),  -- True primary key
    identifier TEXT NOT NULL,                  -- Human-readable ID
    data JSONB NOT NULL,                      -- Flexible document storage
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID NOT NULL,

    -- Business constraints
    CONSTRAINT tb_author_identifier_unique UNIQUE (identifier),
    CONSTRAINT tb_author_data_not_empty CHECK (data IS NOT NULL)
);

COMMENT ON TABLE blog.tb_author IS
'Authors table - content creators and contributors. Uses JSONB data column for flexible profile information.';

-- Posts table - Core content entity
CREATE TABLE blog.tb_post (
    id SERIAL,
    pk_post UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    identifier TEXT NOT NULL,                  -- URL slug
    fk_author UUID NOT NULL REFERENCES blog.tb_author(pk_author),
    data JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',      -- draft, published, archived
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID NOT NULL,

    -- Business constraints
    CONSTRAINT tb_post_identifier_unique UNIQUE (identifier),
    CONSTRAINT tb_post_status_valid CHECK (status IN ('draft', 'published', 'archived')),
    CONSTRAINT tb_post_published_at_logic CHECK (
        (status = 'published' AND published_at IS NOT NULL) OR
        (status != 'published' AND published_at IS NULL)
    )
);

COMMENT ON TABLE blog.tb_post IS
'Blog posts table - core content entities with publication workflow.';

-- Tags table - Content categorization
CREATE TABLE blog.tb_tag (
    id SERIAL,
    pk_tag UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    identifier TEXT NOT NULL,                  -- URL slug
    fk_parent_tag UUID REFERENCES blog.tb_tag(pk_tag), -- Hierarchical tags
    data JSONB NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID NOT NULL,

    -- Business constraints
    CONSTRAINT tb_tag_identifier_unique UNIQUE (identifier),
    CONSTRAINT tb_tag_no_self_reference CHECK (pk_tag != fk_parent_tag),
    CONSTRAINT tb_tag_usage_count_positive CHECK (usage_count >= 0)
);

COMMENT ON TABLE blog.tb_tag IS
'Tags table - hierarchical content categorization with usage tracking.';

-- Post-Tag association table
CREATE TABLE blog.tb_post_tag (
    pk_post_tag UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fk_post UUID NOT NULL REFERENCES blog.tb_post(pk_post) ON DELETE CASCADE,
    fk_tag UUID NOT NULL REFERENCES blog.tb_tag(pk_tag) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,

    -- Unique association
    CONSTRAINT tb_post_tag_unique_pair UNIQUE (fk_post, fk_tag)
);

-- Comments table - User interactions
CREATE TABLE blog.tb_comment (
    id SERIAL,
    pk_comment UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fk_post UUID NOT NULL REFERENCES blog.tb_post(pk_post) ON DELETE CASCADE,
    fk_parent_comment UUID REFERENCES blog.tb_comment(pk_comment), -- Threading
    fk_author UUID REFERENCES blog.tb_author(pk_author), -- Optional for anonymous
    data JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',    -- pending, approved, spam, deleted
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID NOT NULL,

    -- Business constraints
    CONSTRAINT tb_comment_status_valid CHECK (status IN ('pending', 'approved', 'spam', 'deleted')),
    CONSTRAINT tb_comment_no_self_reference CHECK (pk_comment != fk_parent_comment)
);

COMMENT ON TABLE blog.tb_comment IS
'Comments table - threaded discussions with moderation workflow.';

-- ============================================================================
-- INDEXES - Performance optimization
-- ============================================================================

-- Author indexes
CREATE INDEX idx_tb_author_identifier ON blog.tb_author(identifier);
CREATE INDEX idx_tb_author_created_at ON blog.tb_author(created_at DESC);

-- Post indexes
CREATE INDEX idx_tb_post_identifier ON blog.tb_post(identifier);
CREATE INDEX idx_tb_post_author ON blog.tb_post(fk_author);
CREATE INDEX idx_tb_post_status ON blog.tb_post(status);
CREATE INDEX idx_tb_post_published_at ON blog.tb_post(published_at DESC) WHERE status = 'published';
CREATE INDEX idx_tb_post_data_gin ON blog.tb_post USING GIN(data);

-- Tag indexes
CREATE INDEX idx_tb_tag_identifier ON blog.tb_tag(identifier);
CREATE INDEX idx_tb_tag_parent ON blog.tb_tag(fk_parent_tag);
CREATE INDEX idx_tb_tag_usage_count ON blog.tb_tag(usage_count DESC);

-- Post-tag association indexes
CREATE INDEX idx_tb_post_tag_post ON blog.tb_post_tag(fk_post);
CREATE INDEX idx_tb_post_tag_tag ON blog.tb_post_tag(fk_tag);

-- Comment indexes
CREATE INDEX idx_tb_comment_post ON blog.tb_comment(fk_post);
CREATE INDEX idx_tb_comment_parent ON blog.tb_comment(fk_parent_comment);
CREATE INDEX idx_tb_comment_author ON blog.tb_comment(fk_author);
CREATE INDEX idx_tb_comment_status ON blog.tb_comment(status);
CREATE INDEX idx_tb_comment_created_at ON blog.tb_comment(created_at DESC);

-- ============================================================================
-- QUERY SIDE - Materialized Tables (tv_*) for API consumption
-- ============================================================================

-- Materialized authors with denormalized data
CREATE TABLE tv_author (
    id UUID PRIMARY KEY,                       -- Same as pk_author
    identifier TEXT NOT NULL,
    data JSONB NOT NULL,
    post_count INTEGER NOT NULL DEFAULT 0,
    last_post_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

COMMENT ON TABLE tv_author IS
'Materialized author view with post statistics. Updated by refresh functions.';

-- Materialized posts with full author and tag information
CREATE TABLE tv_post (
    id UUID PRIMARY KEY,                       -- Same as pk_post
    identifier TEXT NOT NULL,
    author_id UUID NOT NULL,
    data JSONB NOT NULL,                      -- Includes author and tags data
    status TEXT NOT NULL,
    published_at TIMESTAMPTZ,
    comment_count INTEGER NOT NULL DEFAULT 0,
    tag_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

COMMENT ON TABLE tv_post IS
'Materialized post view with denormalized author and tag data.';

-- Materialized tags with hierarchy and usage stats
CREATE TABLE tv_tag (
    id UUID PRIMARY KEY,                       -- Same as pk_tag
    identifier TEXT NOT NULL,
    parent_id UUID,
    data JSONB NOT NULL,                      -- Includes hierarchy path
    usage_count INTEGER NOT NULL DEFAULT 0,
    post_count INTEGER NOT NULL DEFAULT 0,
    children_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

COMMENT ON TABLE tv_tag IS
'Materialized tag view with hierarchy information and usage statistics.';

-- Materialized comments with threading information
CREATE TABLE tv_comment (
    id UUID PRIMARY KEY,                       -- Same as pk_comment
    post_id UUID NOT NULL,
    parent_id UUID,
    author_id UUID,
    data JSONB NOT NULL,                      -- Includes thread path and author info
    status TEXT NOT NULL,
    reply_count INTEGER NOT NULL DEFAULT 0,
    thread_depth INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

COMMENT ON TABLE tv_comment IS
'Materialized comment view with threading and author information.';

-- ============================================================================
-- REAL-TIME VIEWS (v_*) - Live data for immediate consistency
-- ============================================================================

-- Live author view
CREATE VIEW v_author AS
SELECT
    pk_author AS id,
    identifier,
    data,
    created_at,
    created_by,
    updated_at,
    updated_by
FROM blog.tb_author;

-- Live post view with basic joins
CREATE VIEW v_post AS
SELECT
    p.pk_post AS id,
    p.identifier,
    p.fk_author AS author_id,
    p.data,
    p.status,
    p.published_at,
    p.created_at,
    p.created_by,
    p.updated_at,
    p.updated_by,
    a.identifier AS author_identifier,
    a.data->>'name' AS author_name
FROM blog.tb_post p
JOIN blog.tb_author a ON p.fk_author = a.pk_author;

-- Live tag view
CREATE VIEW v_tag AS
SELECT
    pk_tag AS id,
    identifier,
    fk_parent_tag AS parent_id,
    data,
    usage_count,
    created_at,
    created_by,
    updated_at,
    updated_by
FROM blog.tb_tag;

-- Live comment view with basic threading
CREATE VIEW v_comment AS
SELECT
    pk_comment AS id,
    fk_post AS post_id,
    fk_parent_comment AS parent_id,
    fk_author AS author_id,
    data,
    status,
    created_at,
    created_by,
    updated_at,
    updated_by
FROM blog.tb_comment;

-- ============================================================================
-- MUTATION RESULT TYPE - Following PrintOptim patterns
-- ============================================================================

CREATE TYPE app.mutation_result AS (
    id UUID,                    -- Entity primary key
    updated_fields TEXT[],      -- Fields that were modified
    status TEXT,                -- Operation status (new, updated, noop:*)
    message TEXT,               -- Human-readable message
    object_data JSONB,          -- Complete entity snapshot after mutation
    extra_metadata JSONB        -- Additional context and debugging info
);

COMMENT ON TYPE app.mutation_result IS
'Standardized mutation result type providing consistent return structure for all mutations.';

-- ============================================================================
-- INPUT TYPES - Structured inputs for functions
-- ============================================================================

-- Author creation input
CREATE TYPE app.type_author_input AS (
    identifier TEXT,
    name TEXT,
    email TEXT,
    bio TEXT,
    avatar_url TEXT,
    social_links JSONB
);

-- Post creation input
CREATE TYPE app.type_post_input AS (
    identifier TEXT,            -- URL slug
    title TEXT,
    content TEXT,
    excerpt TEXT,
    featured_image_url TEXT,
    author_identifier TEXT,     -- Reference to author by identifier
    tag_identifiers TEXT[],     -- Array of tag identifiers
    status TEXT,
    publish_at TIMESTAMPTZ
);

-- Tag creation input
CREATE TYPE app.type_tag_input AS (
    identifier TEXT,            -- URL slug
    name TEXT,
    description TEXT,
    color TEXT,
    parent_identifier TEXT      -- Reference to parent tag
);

-- Comment creation input
CREATE TYPE app.type_comment_input AS (
    post_identifier TEXT,       -- Reference to post
    parent_comment_id UUID,     -- For threading
    author_identifier TEXT,     -- Optional author
    content TEXT,
    author_name TEXT,           -- For anonymous comments
    author_email TEXT           -- For anonymous comments
);

-- ============================================================================
-- UTILITY FUNCTIONS - Supporting mutation operations
-- ============================================================================

-- Function to sanitize and validate JSONB input
CREATE OR REPLACE FUNCTION core.sanitize_jsonb_input(input_data JSONB)
RETURNS JSONB
LANGUAGE plpgsql AS $$
BEGIN
    -- Remove null fields and validate structure
    RETURN coalesce(input_data - 'null', '{}'::jsonb);
END;
$$;

COMMENT ON FUNCTION core.sanitize_jsonb_input IS
'Sanitizes JSONB input by removing null fields and ensuring valid structure.';

-- Function to generate URL-safe identifier from text
CREATE OR REPLACE FUNCTION core.generate_identifier(input_text TEXT, max_length INTEGER DEFAULT 50)
RETURNS TEXT
LANGUAGE plpgsql AS $$
BEGIN
    RETURN substring(
        lower(
            regexp_replace(
                regexp_replace(input_text, '[^a-zA-Z0-9\s-]', '', 'g'),
                '\s+', '-', 'g'
            )
        ),
        1, max_length
    );
END;
$$;

COMMENT ON FUNCTION core.generate_identifier IS
'Generates URL-safe identifier from input text by removing special characters and replacing spaces with hyphens.';

-- ============================================================================
-- INDEXES FOR MATERIALIZED TABLES
-- ============================================================================

-- tv_author indexes
CREATE INDEX idx_tv_author_identifier ON tv_author(identifier);
CREATE INDEX idx_tv_author_post_count ON tv_author(post_count DESC);
CREATE INDEX idx_tv_author_last_post_at ON tv_author(last_post_at DESC);

-- tv_post indexes
CREATE INDEX idx_tv_post_identifier ON tv_post(identifier);
CREATE INDEX idx_tv_post_author_id ON tv_post(author_id);
CREATE INDEX idx_tv_post_status ON tv_post(status);
CREATE INDEX idx_tv_post_published_at ON tv_post(published_at DESC) WHERE status = 'published';
CREATE INDEX idx_tv_post_data_gin ON tv_post USING GIN(data);

-- tv_tag indexes
CREATE INDEX idx_tv_tag_identifier ON tv_tag(identifier);
CREATE INDEX idx_tv_tag_parent_id ON tv_tag(parent_id);
CREATE INDEX idx_tv_tag_usage_count ON tv_tag(usage_count DESC);
CREATE INDEX idx_tv_tag_post_count ON tv_tag(post_count DESC);

-- tv_comment indexes
CREATE INDEX idx_tv_comment_post_id ON tv_comment(post_id);
CREATE INDEX idx_tv_comment_parent_id ON tv_comment(parent_id);
CREATE INDEX idx_tv_comment_author_id ON tv_comment(author_id);
CREATE INDEX idx_tv_comment_status ON tv_comment(status);
CREATE INDEX idx_tv_comment_thread_depth ON tv_comment(thread_depth);

-- ============================================================================
-- SAMPLE DATA TYPES for Error Testing
-- ============================================================================

-- Error scenarios for testing
CREATE TYPE core.error_scenario AS ENUM (
    'duplicate_identifier',
    'missing_author',
    'invalid_parent_tag',
    'invalid_post_reference',
    'circular_tag_hierarchy',
    'invalid_status_transition',
    'missing_required_field',
    'invalid_date_range',
    'content_too_long',
    'invalid_email_format'
);

COMMENT ON TYPE core.error_scenario IS
'Enumeration of error scenarios for comprehensive error testing.';
