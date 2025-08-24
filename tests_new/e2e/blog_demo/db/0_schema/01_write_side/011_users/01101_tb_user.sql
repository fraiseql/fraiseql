-- Users command table (tb_user)
-- Stores user account information and profile data

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

-- Audit trigger
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
