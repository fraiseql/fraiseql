-- FraiseQL integration test data for SQL Server
-- Executed by the sqlserver-init one-shot container

-- Create the test database
IF NOT EXISTS (SELECT name FROM sys.databases WHERE name = 'fraiseql_test')
    CREATE DATABASE fraiseql_test;
GO

USE fraiseql_test;
GO

-- ============================================================================
-- Users table + v_user view
-- ============================================================================
CREATE TABLE users (
    id    INT IDENTITY(1,1) PRIMARY KEY,
    data  NVARCHAR(MAX) NOT NULL
        CHECK (ISJSON(data) = 1)
);
GO

INSERT INTO users (data) VALUES
('{"id": 1, "name": "Alice",   "email": "alice@example.com",   "role": "admin",     "age": 28, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
('{"id": 2, "name": "Bob",     "email": "bob@example.com",     "role": "user",      "age": 25, "active": true,  "metadata": {"city": "London",   "country": "GB"}}'),
('{"id": 3, "name": "Charlie", "email": "charlie@example.com", "role": "moderator", "age": 35, "active": false, "metadata": {"city": "Berlin",   "country": "DE"}}'),
('{"id": 4, "name": "Diana",   "email": "diana@example.com",   "role": "user",      "age": 30, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
('{"id": 5, "name": "Eve",     "email": "eve@example.com",     "role": "admin",     "age": 22, "active": true,  "metadata": {"city": "New York", "country": "US"}}');
GO

CREATE VIEW v_user AS SELECT data FROM users;
GO

-- ============================================================================
-- Posts table + v_post view
-- ============================================================================
CREATE TABLE posts (
    id    INT IDENTITY(1,1) PRIMARY KEY,
    data  NVARCHAR(MAX) NOT NULL
        CHECK (ISJSON(data) = 1)
);
GO

INSERT INTO posts (data) VALUES
('{"id": 1, "title": "Hello World",     "author": {"id": 1, "name": "Alice", "email": "alice@example.com"},   "published": true,  "views": 100}'),
('{"id": 2, "title": "GraphQL Basics",  "author": {"id": 2, "name": "Bob",   "email": "bob@example.com"},     "published": true,  "views": 250}'),
('{"id": 3, "title": "Advanced Queries","author": {"id": 1, "name": "Alice", "email": "alice@example.com"},   "published": true,  "views": 75}'),
('{"id": 4, "title": "Draft Post",      "author": {"id": 3, "name": "Charlie","email": "charlie@example.com"},"published": false, "views": 0}');
GO

CREATE VIEW v_post AS SELECT data FROM posts;
GO

-- ============================================================================
-- relay_item table + v_relay_item view  (FraiseQL trinity pattern: UUID pk)
--
-- Used by relay cursor pagination integration tests.
-- 10 rows with a 'score' field to exercise custom order_by and cursor logic.
--
-- UUIDs of the form 00000000-0000-0000-0000-00000000000N are chosen so that
-- SQL Server UNIQUEIDENTIFIER sort order (bytes 10-15 compared first) matches
-- standard lexicographic order for these values: item-1 < item-2 < … < item-10.
-- ============================================================================
CREATE TABLE relay_item (
    id   UNIQUEIDENTIFIER NOT NULL PRIMARY KEY,
    data NVARCHAR(MAX) NOT NULL CHECK (ISJSON(data) = 1)
);
GO

INSERT INTO relay_item (id, data) VALUES
  ('00000000-0000-0000-0000-000000000001', '{"score": 50, "label": "item-1"}'),
  ('00000000-0000-0000-0000-000000000002', '{"score": 30, "label": "item-2"}'),
  ('00000000-0000-0000-0000-000000000003', '{"score": 70, "label": "item-3"}'),
  ('00000000-0000-0000-0000-000000000004', '{"score": 10, "label": "item-4"}'),
  ('00000000-0000-0000-0000-000000000005', '{"score": 90, "label": "item-5"}'),
  ('00000000-0000-0000-0000-000000000006', '{"score": 20, "label": "item-6"}'),
  ('00000000-0000-0000-0000-000000000007', '{"score": 60, "label": "item-7"}'),
  ('00000000-0000-0000-0000-000000000008', '{"score": 40, "label": "item-8"}'),
  ('00000000-0000-0000-0000-000000000009', '{"score": 80, "label": "item-9"}'),
  ('00000000-0000-0000-0000-00000000000a', '{"score": 15, "label": "item-10"}');
GO

CREATE VIEW v_relay_item AS SELECT id, data FROM relay_item;
GO

-- ============================================================================
-- Scored item (window function + CTE + aggregation tests)
--
-- Plain relational columns so RANK(), ROW_NUMBER(), SUM(), COUNT() work
-- without JSON_VALUE. SQL Server 2012+ supports all window functions.
-- ============================================================================
CREATE TABLE score_item (
    id       CHAR(36)     NOT NULL PRIMARY KEY,
    category NVARCHAR(50) NOT NULL,
    score    INT          NOT NULL,
    label    NVARCHAR(100) NOT NULL
);
GO

INSERT INTO score_item (id, category, score, label) VALUES
  ('sc-01', 'A', 95, 'alpha'),
  ('sc-02', 'A', 80, 'beta'),
  ('sc-03', 'A', 80, 'gamma'),
  ('sc-04', 'B', 70, 'delta'),
  ('sc-05', 'B', 60, 'epsilon'),
  ('sc-06', 'B', 90, 'zeta'),
  ('sc-07', 'C', 50, 'eta'),
  ('sc-08', 'C', 55, 'theta');
GO

CREATE VIEW v_score AS SELECT id, category, score, label FROM score_item;
GO

-- ============================================================================
-- Mutation stored procedure
--
-- SQL Server uses OUTPUT INSERTED.* for returning inserted rows.
-- ============================================================================
CREATE TABLE tag_item (
    pk_tag  INT          NOT NULL IDENTITY(1,1) PRIMARY KEY,
    name    NVARCHAR(200) NOT NULL UNIQUE
);
GO

CREATE VIEW v_tag AS SELECT pk_tag, name FROM tag_item;
GO

CREATE OR ALTER PROCEDURE fn_create_tag @p_name NVARCHAR(200)
AS
BEGIN
    SET NOCOUNT ON;
    IF NOT EXISTS (SELECT 1 FROM tag_item WHERE name = @p_name)
        INSERT INTO tag_item (name) OUTPUT INSERTED.pk_tag AS id, INSERTED.name
        VALUES (@p_name);
    ELSE
        SELECT pk_tag AS id, name FROM tag_item WHERE name = @p_name;
END;
GO
