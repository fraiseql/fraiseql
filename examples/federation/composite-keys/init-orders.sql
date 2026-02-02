-- Multi-tenant orders database with composite keys (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_order CASCADE;

CREATE TABLE tb_order (
  pk_tenant_order SERIAL PRIMARY KEY,
  id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
  organization_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  status VARCHAR(50) NOT NULL DEFAULT 'pending',
  amount DECIMAL(10, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(organization_id, order_id)
);

CREATE INDEX idx_tb_org_order ON tb_order(organization_id, order_id);
CREATE INDEX idx_tb_org_user ON tb_order(organization_id, user_id);
CREATE INDEX idx_tb_status ON tb_order(status);
CREATE INDEX idx_tb_order_id ON tb_order(id);

-- Create view (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_order AS
SELECT
    pk_tenant_order,
    jsonb_build_object(
        'id', id,
        'organization_id', organization_id,
        'order_id', order_id,
        'user_id', user_id,
        'status', status,
        'amount', amount,
        'created_at', created_at
    ) AS data
FROM tb_order;

-- Organization 1 orders
INSERT INTO tb_order (organization_id, order_id, user_id, status, amount) VALUES
  ('org1', 'order1', 'user1', 'completed', 149.99),
  ('org1', 'order2', 'user1', 'pending', 299.99),
  ('org1', 'order3', 'user2', 'shipped', 75.50),
  ('org1', 'order4', 'user2', 'completed', 199.99),
  ('org1', 'order5', 'user3', 'pending', 450.00);

-- Organization 2 orders
INSERT INTO tb_order (organization_id, order_id, user_id, status, amount) VALUES
  ('org2', 'order1', 'user1', 'completed', 1200.00),
  ('org2', 'order2', 'user1', 'shipped', 850.50),
  ('org2', 'order3', 'user2', 'pending', 500.00),
  ('org2', 'order4', 'user2', 'completed', 3500.00),
  ('org2', 'order5', 'user1', 'pending', 2200.00);
