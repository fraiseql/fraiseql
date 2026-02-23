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
    customer_id TEXT        NOT NULL,
    product_id  TEXT        NOT NULL,
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
  ('cust-1', 'prod-1',  99.99, 1, 60.00, 0.00,  '{"category": "electronics", "region": "north"}'),
  ('cust-2', 'prod-2', 149.99, 2, 80.00, 5.00,  '{"category": "clothing",    "region": "south"}'),
  ('cust-3', 'prod-1', 199.99, 3, 60.00, 10.00, '{"category": "electronics", "region": "east"}'),
  ('cust-1', 'prod-3',  49.99, 1, 20.00, 0.00,  '{"category": "books",       "region": "west"}'),
  ('cust-4', 'prod-2', 299.99, 4, 80.00, 15.00, '{"category": "clothing",    "region": "north"}')
ON CONFLICT DO NOTHING;
