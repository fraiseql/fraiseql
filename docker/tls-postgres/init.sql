-- FraiseQL TLS Test Database Initialization
-- This creates test data for the TLS integration tests

-- Create test entity table
CREATE TABLE IF NOT EXISTS test_entity (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Insert test data
INSERT INTO test_entity (data)
SELECT jsonb_build_object(
    'name', 'Test Entity ' || i,
    'value', i * 10,
    'active', i % 2 = 0
)
FROM generate_series(1, 100) AS i;

-- Create the view that fraiseql-wire expects (single JSONB column named 'data')
CREATE OR REPLACE VIEW v_test_entity AS
SELECT data FROM test_entity;

-- Create pg_tables view compatible with fraiseql-wire
CREATE OR REPLACE VIEW pg_tables AS
SELECT jsonb_build_object(
    'schemaname', schemaname,
    'tablename', tablename,
    'tableowner', tableowner
) AS data
FROM pg_catalog.pg_tables
WHERE schemaname NOT IN ('pg_catalog', 'information_schema');

-- Create pg_version view
CREATE OR REPLACE VIEW pg_version AS
SELECT jsonb_build_object('version', version()) AS data;

-- Grant permissions
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO fraiseql;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO fraiseql;
