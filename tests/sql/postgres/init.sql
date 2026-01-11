-- PostgreSQL Test Database Initialization
--
-- This script creates test views with JSONB data for FraiseQL integration tests.

-- Enable necessary extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- Test View: v_user
-- ============================================================================

CREATE TABLE IF NOT EXISTS users_test (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email TEXT NOT NULL,
    name TEXT NOT NULL,
    age INTEGER,
    active BOOLEAN DEFAULT true,
    role TEXT DEFAULT 'user',
    tags TEXT[] DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT NOW(),
    deleted_at TIMESTAMP
);

-- Insert test data
INSERT INTO users_test (id, email, name, age, active, role, tags, metadata) VALUES
    ('00000000-0000-0000-0000-000000000001', 'alice@example.com', 'Alice Smith', 30, true, 'admin', ARRAY['admin', 'developer'], '{"city": "Paris", "country": "France"}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'bob@example.com', 'Bob Jones', 25, true, 'user', ARRAY['user'], '{"city": "London", "country": "UK"}'::jsonb),
    ('00000000-0000-0000-0000-000000000003', 'charlie@test.com', 'Charlie Brown', 35, false, 'moderator', ARRAY['moderator'], '{"city": "New York", "country": "USA"}'::jsonb),
    ('00000000-0000-0000-0000-000000000004', 'diana@example.com', 'Diana Prince', 28, true, 'user', ARRAY['user', 'premium'], '{"city": "Berlin", "country": "Germany"}'::jsonb),
    ('00000000-0000-0000-0000-000000000005', 'eve@test.com', 'Eve Wilson', 22, true, 'user', ARRAY['user'], '{"city": "Tokyo", "country": "Japan"}'::jsonb)
ON CONFLICT DO NOTHING;

-- Create JSONB view (FraiseQL's simplified execution model)
CREATE OR REPLACE VIEW v_user AS
SELECT
    jsonb_build_object(
        'id', id::text,
        'email', email,
        'name', name,
        'age', age,
        'active', active,
        'role', role,
        'tags', to_jsonb(tags),
        'metadata', metadata,
        'created_at', created_at,
        'deleted_at', deleted_at
    ) AS data
FROM users_test;

-- ============================================================================
-- Test View: v_post
-- ============================================================================

CREATE TABLE IF NOT EXISTS posts_test (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title TEXT NOT NULL,
    content TEXT,
    author_id UUID REFERENCES users_test(id),
    published BOOLEAN DEFAULT false,
    views INTEGER DEFAULT 0,
    tags TEXT[] DEFAULT '{}',
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert test data
INSERT INTO posts_test (id, title, content, author_id, published, views, tags) VALUES
    ('00000000-0000-0000-0000-000000000101', 'Introduction to GraphQL', 'GraphQL is a query language...', '00000000-0000-0000-0000-000000000001', true, 150, ARRAY['graphql', 'tutorial']),
    ('00000000-0000-0000-0000-000000000102', 'Rust Performance', 'Rust offers zero-cost abstractions...', '00000000-0000-0000-0000-000000000001', true, 200, ARRAY['rust', 'performance']),
    ('00000000-0000-0000-0000-000000000103', 'Draft Post', 'This is a draft...', '00000000-0000-0000-0000-000000000002', false, 0, ARRAY['draft']),
    ('00000000-0000-0000-0000-000000000104', 'PostgreSQL Tips', 'JSONB is powerful...', '00000000-0000-0000-0000-000000000003', true, 75, ARRAY['postgresql', 'database'])
ON CONFLICT DO NOTHING;

-- Create JSONB view
CREATE OR REPLACE VIEW v_post AS
SELECT
    jsonb_build_object(
        'id', p.id::text,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT jsonb_build_object(
                'id', u.id::text,
                'name', u.name,
                'email', u.email
            )
            FROM users_test u
            WHERE u.id = p.author_id
        ),
        'published', p.published,
        'views', p.views,
        'tags', to_jsonb(p.tags),
        'created_at', p.created_at
    ) AS data
FROM posts_test p;

-- ============================================================================
-- Test View: v_product (for testing numeric/comparison operators)
-- ============================================================================

CREATE TABLE IF NOT EXISTS products_test (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    price NUMERIC(10, 2) NOT NULL,
    stock INTEGER DEFAULT 0,
    category TEXT,
    attributes JSONB DEFAULT '{}'
);

-- Insert test data
INSERT INTO products_test (id, name, price, stock, category, attributes) VALUES
    ('00000000-0000-0000-0000-000000000201', 'Laptop', 999.99, 10, 'electronics', '{"brand": "Dell", "screen": 15}'::jsonb),
    ('00000000-0000-0000-0000-000000000202', 'Mouse', 29.99, 50, 'electronics', '{"brand": "Logitech", "wireless": true}'::jsonb),
    ('00000000-0000-0000-0000-000000000203', 'Desk', 299.99, 5, 'furniture', '{"material": "wood", "adjustable": false}'::jsonb),
    ('00000000-0000-0000-0000-000000000204', 'Chair', 199.99, 15, 'furniture', '{"material": "leather", "adjustable": true}'::jsonb)
ON CONFLICT DO NOTHING;

-- Create JSONB view
CREATE OR REPLACE VIEW v_product AS
SELECT
    jsonb_build_object(
        'id', id::text,
        'name', name,
        'price', price,
        'stock', stock,
        'category', category,
        'attributes', attributes
    ) AS data
FROM products_test;

-- ============================================================================
-- Grants
-- ============================================================================

GRANT SELECT ON v_user TO fraiseql_test;
GRANT SELECT ON v_post TO fraiseql_test;
GRANT SELECT ON v_product TO fraiseql_test;

-- ============================================================================
-- Verification
-- ============================================================================

-- Verify views are created
SELECT 'v_user' AS view_name, COUNT(*) AS row_count FROM v_user
UNION ALL
SELECT 'v_post', COUNT(*) FROM v_post
UNION ALL
SELECT 'v_product', COUNT(*) FROM v_product;
