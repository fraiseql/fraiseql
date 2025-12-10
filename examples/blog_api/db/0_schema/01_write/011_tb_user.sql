CREATE TABLE IF NOT EXISTS tb_user (
    -- Trinity Identifiers
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,         -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                           -- Human-readable (email/username)

    -- User data
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_user IS 'Users table with Trinity pattern: pk_user (INT, internal), id (UUID, public API), identifier (TEXT, username)';
COMMENT ON COLUMN tb_user.pk_user IS 'Internal primary key (INT) for fast database joins - NOT exposed in GraphQL';
COMMENT ON COLUMN tb_user.id IS 'Public UUID identifier for GraphQL API - secure, prevents enumeration';
COMMENT ON COLUMN tb_user.identifier IS 'Human-readable username for SEO-friendly URLs';

CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier);
CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_active ON tb_user(is_active);
