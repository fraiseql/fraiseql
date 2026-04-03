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

-- ============================================================================
-- Relay pagination item
--
-- Used by keyset pagination integration tests.
-- 10 rows with deterministic CHAR(36) UUIDs in lexicographic ascending order.
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_relay_item (
    id   CHAR(36) NOT NULL PRIMARY KEY,
    data JSON     NOT NULL
);

CREATE OR REPLACE VIEW v_relay_item AS
SELECT id, data FROM tb_relay_item;

INSERT IGNORE INTO tb_relay_item (id, data) VALUES
  ('00000000-0000-0000-0000-000000000001', '{"id": "00000000-0000-0000-0000-000000000001", "score": 50, "label": "item-1"}'),
  ('00000000-0000-0000-0000-000000000002', '{"id": "00000000-0000-0000-0000-000000000002", "score": 30, "label": "item-2"}'),
  ('00000000-0000-0000-0000-000000000003', '{"id": "00000000-0000-0000-0000-000000000003", "score": 70, "label": "item-3"}'),
  ('00000000-0000-0000-0000-000000000004', '{"id": "00000000-0000-0000-0000-000000000004", "score": 10, "label": "item-4"}'),
  ('00000000-0000-0000-0000-000000000005', '{"id": "00000000-0000-0000-0000-000000000005", "score": 90, "label": "item-5"}'),
  ('00000000-0000-0000-0000-000000000006', '{"id": "00000000-0000-0000-0000-000000000006", "score": 20, "label": "item-6"}'),
  ('00000000-0000-0000-0000-000000000007', '{"id": "00000000-0000-0000-0000-000000000007", "score": 60, "label": "item-7"}'),
  ('00000000-0000-0000-0000-000000000008', '{"id": "00000000-0000-0000-0000-000000000008", "score": 40, "label": "item-8"}'),
  ('00000000-0000-0000-0000-000000000009', '{"id": "00000000-0000-0000-0000-000000000009", "score": 80, "label": "item-9"}'),
  ('00000000-0000-0000-0000-00000000000a', '{"id": "00000000-0000-0000-0000-00000000000a", "score": 15, "label": "item-10"}');

-- ============================================================================
-- Scored item (window function + CTE + aggregation tests)
--
-- Plain relational columns so SQL aggregation functions work directly without
-- JSON_EXTRACT. MySQL 8+ supports RANK() and ROW_NUMBER() window functions.
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_score (
    id       CHAR(36)     NOT NULL PRIMARY KEY,
    category VARCHAR(50)  NOT NULL,
    score    INT          NOT NULL,
    label    VARCHAR(100) NOT NULL
);

CREATE OR REPLACE VIEW v_score AS
SELECT id, category, score, label FROM tb_score;

INSERT IGNORE INTO tb_score (id, category, score, label) VALUES
  ('sc-01', 'A', 95, 'alpha'),
  ('sc-02', 'A', 80, 'beta'),
  ('sc-03', 'A', 80, 'gamma'),
  ('sc-04', 'B', 70, 'delta'),
  ('sc-05', 'B', 60, 'epsilon'),
  ('sc-06', 'B', 90, 'zeta'),
  ('sc-07', 'C', 50, 'eta'),
  ('sc-08', 'C', 55, 'theta');

-- ============================================================================
-- Mutation stored procedure
--
-- MySQL does not support RETURNING. Mutations use stored procedures.
-- Inserts a tag and returns the new row via SELECT after INSERT.
-- ============================================================================

CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag  INT          NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name    VARCHAR(200) NOT NULL UNIQUE
);

CREATE OR REPLACE VIEW v_tag AS
SELECT pk_tag, name FROM tb_tag;

-- Stored procedure is loaded separately (see procedures.sql) because
-- DELIMITER directives are a MySQL CLI feature that may not work reliably
-- when the script is piped via stdin in batch mode.
