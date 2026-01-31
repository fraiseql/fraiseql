-- Arrow-optimized ta_users materialized table
--
-- This table stores flattened user data with scalar columns for efficient
-- Arrow Flight queries. Values are extracted from source tb_user JSONB data.

CREATE TABLE IF NOT EXISTS ta_users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    source_updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create BRIN index for efficient time-series queries
CREATE INDEX IF NOT EXISTS idx_ta_users_created_at
    ON ta_users USING BRIN (created_at);

-- Populate with test data
INSERT INTO ta_users (id, name, email, created_at)
VALUES
    ('user-1', 'Alice Johnson', 'alice@example.com', NOW()),
    ('user-2', 'Bob Smith', 'bob@example.com', NOW() - INTERVAL '1 day'),
    ('user-3', 'Charlie Brown', 'charlie@example.com', NOW() - INTERVAL '2 days'),
    ('user-4', 'Diana Prince', 'diana@example.com', NOW() - INTERVAL '3 days'),
    ('user-5', 'Eve Wilson', 'eve@example.com', NOW() - INTERVAL '4 days')
ON CONFLICT (id) DO NOTHING;
