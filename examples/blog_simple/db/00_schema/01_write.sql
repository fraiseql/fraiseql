-- ==============================================================================
-- WRITE SIDE (Command): All tb_* tables
-- ==============================================================================
-- File: 00_schema/01_write.sql
-- Layer: Write (command side)
-- Contains: tb_user, tb_post, tb_comment - normalized, source of truth
-- ==============================================================================

-- User table (command side)
CREATE TABLE tb_user (
    -- Trinity pattern identifiers
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,

    -- User data
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    bio TEXT,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Post table (command side)
CREATE TABLE tb_post (
    -- Trinity pattern identifiers
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,  -- slug

    -- Post data
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    published_at TIMESTAMPTZ,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Comment table (command side)
CREATE TABLE tb_comment (
    -- Trinity pattern identifiers
    pk_comment INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,

    -- Comment data
    fk_post INT NOT NULL REFERENCES tb_post(pk_post) ON DELETE CASCADE,
    fk_author INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    fk_parent INT REFERENCES tb_comment(pk_comment) ON DELETE CASCADE,
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
