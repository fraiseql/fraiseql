-- FraiseQL PostgreSQL Analytics Test Schema
--
-- Follows fraiseql naming conventions:
--   tf_{entity} - analytics fact table (measures + JSONB dimensions)

-- ============================================================================
-- Sales Fact Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS tf_sales (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    customer_id UUID        NOT NULL DEFAULT gen_random_uuid(),
    product_id  UUID        NOT NULL DEFAULT gen_random_uuid(),
    revenue     NUMERIC(12, 2) NOT NULL,
    quantity    INTEGER        NOT NULL,
    cost        NUMERIC(12, 2) NOT NULL DEFAULT 0,
    discount    NUMERIC(5, 2)  NOT NULL DEFAULT 0,
    data        JSONB          NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_tf_sales_customer_id  ON tf_sales (customer_id);
CREATE INDEX IF NOT EXISTS idx_tf_sales_product_id   ON tf_sales (product_id);
CREATE INDEX IF NOT EXISTS idx_tf_sales_occurred_at  ON tf_sales (occurred_at);

INSERT INTO tf_sales (customer_id, product_id, revenue, quantity, cost, discount, data) VALUES
  ('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'b1eebc99-9c0b-4ef8-bb6d-6bb9bd380a22',  99.99, 1, 60.00,  0.00, '{"category": "electronics", "region": "north"}'),
  ('a1eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'b2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22', 149.99, 2, 80.00,  5.00, '{"category": "clothing",    "region": "south"}'),
  ('a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'b1eebc99-9c0b-4ef8-bb6d-6bb9bd380a22', 199.99, 3, 60.00, 10.00, '{"category": "electronics", "region": "east"}'),
  ('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'b3eebc99-9c0b-4ef8-bb6d-6bb9bd380a22',  49.99, 1, 20.00,  0.00, '{"category": "books",       "region": "west"}'),
  ('a3eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'b2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22', 299.99, 4, 80.00, 15.00, '{"category": "clothing",    "region": "north"}')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Events Fact Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS tf_events (
    id             BIGSERIAL      PRIMARY KEY,
    occurred_at    TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    created_at     TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    endpoint       VARCHAR(255)   NOT NULL,
    status_code    INTEGER        NOT NULL,
    duration_ms    BIGINT         NOT NULL,
    error_count    INTEGER        NOT NULL DEFAULT 0,
    request_size   BIGINT         NOT NULL DEFAULT 0,
    response_size  BIGINT         NOT NULL DEFAULT 0,
    data           JSONB          NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_tf_events_endpoint    ON tf_events (endpoint);
CREATE INDEX IF NOT EXISTS idx_tf_events_status_code ON tf_events (status_code);
CREATE INDEX IF NOT EXISTS idx_tf_events_occurred_at ON tf_events (occurred_at);

INSERT INTO tf_events
  (endpoint, status_code, duration_ms, error_count, request_size, response_size, data)
VALUES
  ('/api/users',    200,  42, 0, 128, 512,  '{"method": "GET",  "version": "1.0"}'),
  ('/api/orders',   201, 105, 0, 256, 1024, '{"method": "POST", "version": "1.0"}'),
  ('/api/users',    500, 350, 1, 128, 256,  '{"method": "GET",  "version": "1.0"}'),
  ('/api/products', 200,  28, 0,  64, 768,  '{"method": "GET",  "version": "2.0"}'),
  ('/api/orders',   200,  89, 0, 192, 640,  '{"method": "GET",  "version": "2.0"}')
ON CONFLICT DO NOTHING;
