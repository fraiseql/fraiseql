-- PostgreSQL Analytics Test Data
--
-- This script creates fact tables for testing analytics introspection and aggregation.

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

-- ============================================================================
-- Verification
-- ============================================================================

SELECT 'tf_sales' AS table_name, COUNT(*) AS row_count FROM tf_sales
UNION ALL
SELECT 'tf_events', COUNT(*) FROM tf_events
UNION ALL
SELECT 'ta_sales_by_day', COUNT(*) FROM ta_sales_by_day;
