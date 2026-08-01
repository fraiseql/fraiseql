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

-- `role` (singular), `age`, `active` and `metadata` mirror the local compose
-- seed (docker/init/postgres-test.sql). The two rigs had drifted: the
-- fraiseql-db live-PostgreSQL suite filters on those keys, and it ran in NO CI
-- leg until P22 wired it in, so the divergence was invisible. Keys are additive
-- — `roles` is untouched for the suites that read it.
INSERT INTO tb_user (data) VALUES
  ('{"id": "user-1", "name": "Alice Johnson",  "email": "alice@example.com",   "roles": ["admin"],         "role": "admin",     "age": 28, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
  ('{"id": "user-2", "name": "Bob Smith",      "email": "bob@example.com",     "roles": ["user"],          "role": "user",      "age": 25, "active": true,  "metadata": {"city": "London",   "country": "GB"}}'),
  ('{"id": "user-3", "name": "Charlie Brown",  "email": "charlie@example.com", "roles": ["user"],          "role": "moderator", "age": 35, "active": false, "metadata": {"city": "Berlin",   "country": "DE"}}'),
  ('{"id": "user-4", "name": "Diana Prince",   "email": "diana@example.com",   "roles": ["user"],          "role": "user",      "age": 30, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
  ('{"id": "user-5", "name": "Eve Wilson",     "email": "eve@example.com",     "roles": ["admin", "user"], "role": "admin",     "age": 22, "active": true,  "metadata": {"city": "New York", "country": "US"}}')
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

-- Four posts: `test_query_all_posts` asserts the count, and the local compose
-- seed has four. See the tb_user note above on the rig divergence.
INSERT INTO tb_post (data) VALUES
  ('{"id": "post-1", "title": "Hello World",      "author": {"id": "user-1", "name": "Alice Johnson"}, "published": true,  "views": 100}'),
  ('{"id": "post-2", "title": "Getting Started",  "author": {"id": "user-2", "name": "Bob Smith"},     "published": true,  "views": 250}'),
  ('{"id": "post-3", "title": "Advanced Topics",  "author": {"id": "user-1", "name": "Alice Johnson"}, "published": false, "views": 75}'),
  ('{"id": "post-4", "title": "Draft Thoughts",   "author": {"id": "user-3", "name": "Charlie Brown"}, "published": false, "views": 10}')
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
