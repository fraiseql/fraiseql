-- Arrow-optimized ta_orders materialized table
--
-- This table stores flattened order data with scalar columns for efficient
-- Arrow Flight queries. Values are extracted from source tb_order JSONB data.

CREATE TABLE IF NOT EXISTS ta_orders (
    id TEXT PRIMARY KEY,
    total NUMERIC(12, 2) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    customer_name TEXT NOT NULL,
    source_updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create BRIN index for efficient time-series queries
CREATE INDEX IF NOT EXISTS idx_ta_orders_created_at
    ON ta_orders USING BRIN (created_at);

-- Populate with test data
INSERT INTO ta_orders (id, total, created_at, customer_name)
VALUES
    ('order-1', 99.99, NOW(), 'Alice Johnson'),
    ('order-2', 149.99, NOW() - INTERVAL '1 day', 'Bob Smith'),
    ('order-3', 199.99, NOW() - INTERVAL '2 days', 'Charlie Brown'),
    ('order-4', 299.99, NOW() - INTERVAL '3 days', 'Diana Prince'),
    ('order-5', 399.99, NOW() - INTERVAL '4 days', 'Eve Wilson')
ON CONFLICT (id) DO NOTHING;
