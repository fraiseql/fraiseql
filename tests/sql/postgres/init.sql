-- FraiseQL PostgreSQL Integration Test Schema
--
-- Follows fraiseql naming conventions:
--   tb_{entity} - command-side JSONB storage table
--   v_{entity}  - canonical entity view (data plane)

-- ============================================================================
-- Users
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_user (
    id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW v_user AS
SELECT id, data FROM tb_user;

CREATE INDEX IF NOT EXISTS idx_tb_user_id ON tb_user (id);

INSERT INTO tb_user (data) VALUES
  ('{"id": "user-1", "name": "Alice Johnson",  "email": "alice@example.com",   "roles": ["admin"]}'),
  ('{"id": "user-2", "name": "Bob Smith",      "email": "bob@example.com",     "roles": ["user"]}'),
  ('{"id": "user-3", "name": "Charlie Brown",  "email": "charlie@example.com", "roles": ["user"]}'),
  ('{"id": "user-4", "name": "Diana Prince",   "email": "diana@example.com",   "roles": ["user"]}'),
  ('{"id": "user-5", "name": "Eve Wilson",     "email": "eve@example.com",     "roles": ["admin", "user"]}')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Posts (with nested author)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_post (
    id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW v_post AS
SELECT id, data FROM tb_post;

INSERT INTO tb_post (data) VALUES
  ('{"id": "post-1", "title": "Hello World",     "author": {"id": "user-1", "name": "Alice Johnson"}, "published": true}'),
  ('{"id": "post-2", "title": "Getting Started", "author": {"id": "user-2", "name": "Bob Smith"},    "published": true}'),
  ('{"id": "post-3", "title": "Advanced Topics", "author": {"id": "user-1", "name": "Alice Johnson"}, "published": false}')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Orders
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_order (
    id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW v_order AS
SELECT id, data FROM tb_order;

INSERT INTO tb_order (data) VALUES
  ('{"id": "order-1", "customer_id": "user-1", "total": 99.99,  "status": "completed"}'),
  ('{"id": "order-2", "customer_id": "user-2", "total": 149.99, "status": "pending"}'),
  ('{"id": "order-3", "customer_id": "user-3", "total": 199.99, "status": "completed"}')
ON CONFLICT DO NOTHING;
