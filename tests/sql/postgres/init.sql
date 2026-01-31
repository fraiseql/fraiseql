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

-- ============================================================================
-- Fact Table: tf_sales (sales transactions)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tf_sales (
    id BIGSERIAL PRIMARY KEY,

    -- Measures (numeric columns for aggregation)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    discount DECIMAL(10,2) DEFAULT 0.00,

    -- Dimensions (JSONB for flexible grouping)
    data JSONB NOT NULL,

    -- Denormalized filters (indexed for fast WHERE)
    customer_id UUID NOT NULL,
    product_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create indexes for denormalized filters
CREATE INDEX IF NOT EXISTS idx_sales_customer ON tf_sales(customer_id);
CREATE INDEX IF NOT EXISTS idx_sales_product ON tf_sales(product_id);
CREATE INDEX IF NOT EXISTS idx_sales_occurred ON tf_sales(occurred_at);
CREATE INDEX IF NOT EXISTS idx_sales_data_gin ON tf_sales USING GIN(data);

-- Insert test data
INSERT INTO tf_sales (revenue, quantity, cost, discount, data, customer_id, product_id, occurred_at) VALUES
    -- Electronics sales
    (999.99, 1, 700.00, 0.00, '{"category": "electronics", "region": "US", "channel": "online"}'::jsonb,
     '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000201', '2024-01-15 10:30:00+00'),
    (29.99, 2, 15.00, 5.00, '{"category": "electronics", "region": "UK", "channel": "online"}'::jsonb,
     '00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000202', '2024-01-16 14:20:00+00'),
    (999.99, 1, 700.00, 100.00, '{"category": "electronics", "region": "FR", "channel": "store"}'::jsonb,
     '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000201', '2024-01-17 09:15:00+00'),

    -- Furniture sales
    (299.99, 1, 180.00, 0.00, '{"category": "furniture", "region": "US", "channel": "store"}'::jsonb,
     '00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000203', '2024-01-18 11:45:00+00'),
    (199.99, 2, 120.00, 20.00, '{"category": "furniture", "region": "DE", "channel": "online"}'::jsonb,
     '00000000-0000-0000-0000-000000000004', '00000000-0000-0000-0000-000000000204', '2024-01-19 16:30:00+00'),
    (299.99, 1, 180.00, 30.00, '{"category": "furniture", "region": "JP", "channel": "online"}'::jsonb,
     '00000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000203', '2024-01-20 08:00:00+00'),

    -- More electronics
    (29.99, 5, 15.00, 0.00, '{"category": "electronics", "region": "US", "channel": "online"}'::jsonb,
     '00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000202', '2024-01-21 13:25:00+00'),
    (999.99, 1, 700.00, 50.00, '{"category": "electronics", "region": "UK", "channel": "store"}'::jsonb,
     '00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000201', '2024-01-22 10:10:00+00')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Fact Table: tf_events (event logs)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tf_events (
    id BIGSERIAL PRIMARY KEY,

    -- Measures
    duration_ms BIGINT NOT NULL,
    error_count INT DEFAULT 0,
    request_size BIGINT DEFAULT 0,
    response_size BIGINT DEFAULT 0,

    -- Dimensions
    data JSONB NOT NULL,

    -- Denormalized filters
    user_id UUID,
    endpoint VARCHAR(255) NOT NULL,
    status_code INT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_events_user ON tf_events(user_id);
CREATE INDEX IF NOT EXISTS idx_events_endpoint ON tf_events(endpoint);
CREATE INDEX IF NOT EXISTS idx_events_status ON tf_events(status_code);
CREATE INDEX IF NOT EXISTS idx_events_occurred ON tf_events(occurred_at);
CREATE INDEX IF NOT EXISTS idx_events_data_gin ON tf_events USING GIN(data);

-- Insert test data
INSERT INTO tf_events (duration_ms, error_count, request_size, response_size, data, user_id, endpoint, status_code, occurred_at) VALUES
    (150, 0, 512, 2048, '{"method": "GET", "version": "v1", "client": "web"}'::jsonb,
     '00000000-0000-0000-0000-000000000001', '/api/users', 200, '2024-01-15 10:00:00+00'),
    (250, 0, 1024, 4096, '{"method": "POST", "version": "v1", "client": "mobile"}'::jsonb,
     '00000000-0000-0000-0000-000000000002', '/api/users', 201, '2024-01-15 10:05:00+00'),
    (50, 1, 256, 128, '{"method": "GET", "version": "v1", "client": "web"}'::jsonb,
     '00000000-0000-0000-0000-000000000003', '/api/posts', 404, '2024-01-15 10:10:00+00'),
    (180, 0, 768, 3072, '{"method": "GET", "version": "v2", "client": "web"}'::jsonb,
     '00000000-0000-0000-0000-000000000001', '/api/posts', 200, '2024-01-15 10:15:00+00'),
    (5000, 1, 512, 256, '{"method": "POST", "version": "v1", "client": "mobile"}'::jsonb,
     '00000000-0000-0000-0000-000000000004', '/api/orders', 500, '2024-01-15 10:20:00+00')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Non-Fact Table: ta_sales_by_day (aggregate table - for testing rejection)
-- ============================================================================

CREATE TABLE IF NOT EXISTS ta_sales_by_day (
    id BIGSERIAL PRIMARY KEY,
    day DATE NOT NULL UNIQUE,
    total_revenue DECIMAL(10,2) NOT NULL,
    total_quantity INT NOT NULL,
    transaction_count INT NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- ============================================================================
-- Grants
-- ============================================================================

GRANT SELECT ON tf_sales TO fraiseql_test;
GRANT SELECT ON tf_events TO fraiseql_test;
GRANT SELECT ON ta_sales_by_day TO fraiseql_test;
