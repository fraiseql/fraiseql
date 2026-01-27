-- Multi-tenant orders database with composite keys

CREATE TABLE tenant_orders (
  organization_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  status VARCHAR(50) NOT NULL DEFAULT 'pending',
  amount DECIMAL(10, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (organization_id, order_id)
);

CREATE INDEX idx_org_order ON tenant_orders(organization_id, order_id);
CREATE INDEX idx_org_user ON tenant_orders(organization_id, user_id);
CREATE INDEX idx_status ON tenant_orders(status);

-- Organization 1 orders
INSERT INTO tenant_orders (organization_id, order_id, user_id, status, amount) VALUES
  ('org1', 'order1', 'user1', 'completed', 149.99),
  ('org1', 'order2', 'user1', 'pending', 299.99),
  ('org1', 'order3', 'user2', 'shipped', 75.50),
  ('org1', 'order4', 'user2', 'completed', 199.99),
  ('org1', 'order5', 'user3', 'pending', 450.00);

-- Organization 2 orders
INSERT INTO tenant_orders (organization_id, order_id, user_id, status, amount) VALUES
  ('org2', 'order1', 'user1', 'completed', 1200.00),
  ('org2', 'order2', 'user1', 'shipped', 850.50),
  ('org2', 'order3', 'user2', 'pending', 500.00),
  ('org2', 'order4', 'user2', 'completed', 3500.00),
  ('org2', 'order5', 'user1', 'pending', 2200.00);
