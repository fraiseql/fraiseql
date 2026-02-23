-- FraiseQL MySQL Integration Test Schema
--
-- Follows fraiseql naming conventions:
--   tb_{entity} - command-side JSON storage table
--   v_{entity}  - canonical entity view (data plane)

-- ============================================================================
-- Users
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_user (
    id   CHAR(36)    NOT NULL PRIMARY KEY DEFAULT (UUID()),
    data JSON        NOT NULL
);

CREATE OR REPLACE VIEW v_user AS
SELECT id, data FROM tb_user;

INSERT IGNORE INTO tb_user (id, data) VALUES
  ('user-1', '{"id": "user-1", "name": "Alice Johnson",  "email": "alice@example.com",   "roles": ["admin"]}'),
  ('user-2', '{"id": "user-2", "name": "Bob Smith",      "email": "bob@example.com",     "roles": ["user"]}'),
  ('user-3', '{"id": "user-3", "name": "Charlie Brown",  "email": "charlie@example.com", "roles": ["user"]}'),
  ('user-4', '{"id": "user-4", "name": "Diana Prince",   "email": "diana@example.com",   "roles": ["user"]}'),
  ('user-5', '{"id": "user-5", "name": "Eve Wilson",     "email": "eve@example.com",     "roles": ["admin", "user"]}');

-- ============================================================================
-- Posts (with nested author)
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_post (
    id   CHAR(36)    NOT NULL PRIMARY KEY DEFAULT (UUID()),
    data JSON        NOT NULL
);

CREATE OR REPLACE VIEW v_post AS
SELECT id, data FROM tb_post;

INSERT IGNORE INTO tb_post (id, data) VALUES
  ('post-1', '{"id": "post-1", "title": "Hello World",     "author": {"id": "user-1", "name": "Alice Johnson"}, "published": true}'),
  ('post-2', '{"id": "post-2", "title": "Getting Started", "author": {"id": "user-2", "name": "Bob Smith"},    "published": true}'),
  ('post-3', '{"id": "post-3", "title": "Advanced Topics", "author": {"id": "user-1", "name": "Alice Johnson"}, "published": false}');
