-- Trinity Pattern Template
-- Replace with your actual table schema

CREATE TABLE tb_entity (
    -- Trinity Identifiers (required)
    pk_entity INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    identifier TEXT UNIQUE,  -- Optional: human-readable slug

    -- Your business fields here
    name TEXT NOT NULL,
    description TEXT,

    -- Audit fields (recommended)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_tb_entity_id ON tb_entity(id);
CREATE INDEX idx_tb_entity_identifier ON tb_entity(identifier);
