-- FraiseQL SQL Server Integration Test Schema
--
-- Follows fraiseql naming conventions:
--   tb_{entity} - command-side JSON storage table
--   v_{entity}  - canonical entity view (data plane)
--
-- Idempotent: safe to run multiple times (uses IF NOT EXISTS / IF OBJECT_ID guards).

-- ============================================================================
-- Create test databases
-- ============================================================================

IF NOT EXISTS (SELECT name FROM sys.databases WHERE name = N'test_fraiseql')
    CREATE DATABASE test_fraiseql;
GO

IF NOT EXISTS (SELECT name FROM sys.databases WHERE name = N'fraiseql_test')
    CREATE DATABASE fraiseql_test;
GO

-- ============================================================================
-- test_fraiseql: Users schema (used by sqlserver_tests)
-- ============================================================================

USE test_fraiseql;
GO

IF OBJECT_ID('dbo.tb_user', 'U') IS NULL
    CREATE TABLE dbo.tb_user (
        id   NVARCHAR(64)  NOT NULL PRIMARY KEY,
        data NVARCHAR(MAX) NOT NULL
    );
GO

IF OBJECT_ID('dbo.v_user', 'V') IS NOT NULL
    DROP VIEW dbo.v_user;
GO

CREATE VIEW dbo.v_user AS
SELECT id, data FROM dbo.tb_user;
GO

-- Seed users (skip if already present)
IF NOT EXISTS (SELECT 1 FROM dbo.tb_user WHERE id = 'user-1')
BEGIN
    INSERT INTO dbo.tb_user (id, data) VALUES
        ('user-1', '{"id": "user-1", "name": "Alice Johnson",  "email": "alice@example.com",   "roles": ["admin"]}'),
        ('user-2', '{"id": "user-2", "name": "Bob Smith",      "email": "bob@example.com",     "roles": ["user"]}'),
        ('user-3', '{"id": "user-3", "name": "Charlie Brown",  "email": "charlie@example.com", "roles": ["user"]}'),
        ('user-4', '{"id": "user-4", "name": "Diana Prince",   "email": "diana@example.com",   "roles": ["user"]}'),
        ('user-5', '{"id": "user-5", "name": "Eve Wilson",     "email": "eve@example.com",     "roles": ["admin", "user"]}');
END
GO

-- ============================================================================
-- fraiseql_test: Relay pagination schema (used by sqlserver_relay_tests)
-- ============================================================================

USE fraiseql_test;
GO

IF OBJECT_ID('dbo.tb_relay_item', 'U') IS NULL
    CREATE TABLE dbo.tb_relay_item (
        pk_item   BIGINT        IDENTITY(1,1) PRIMARY KEY,
        id        UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID() UNIQUE,
        label     NVARCHAR(255) NOT NULL,
        score     INT           NOT NULL
    );
GO

IF OBJECT_ID('dbo.v_relay_item', 'V') IS NOT NULL
    DROP VIEW dbo.v_relay_item;
GO

-- v_relay_item returns (id, data) as required by the fraiseql view contract.
-- JSON is built inline so the adapter can extract typed fields from the data column.
CREATE VIEW dbo.v_relay_item AS
SELECT
    id,
    (SELECT id, label, score FOR JSON PATH, WITHOUT_ARRAY_WRAPPER) AS data
FROM dbo.tb_relay_item;
GO

-- Seed 10 relay items with predictable UUIDs so cursor tests are deterministic.
-- UUIDs use the form 00000000-0000-0000-0000-00000000000N (N = 1..a).
-- SQL Server compares UNIQUEIDENTIFIER bytes 10-15 first; for these UUIDs those
-- bytes are 000000000001 ... 00000000000a, giving standard ascending order.
IF NOT EXISTS (SELECT 1 FROM dbo.tb_relay_item WHERE id = '00000000-0000-0000-0000-000000000001')
BEGIN
    SET IDENTITY_INSERT dbo.tb_relay_item ON;
    INSERT INTO dbo.tb_relay_item (pk_item, id, label, score) VALUES
        ( 1, '00000000-0000-0000-0000-000000000001', 'item-1',  50),
        ( 2, '00000000-0000-0000-0000-000000000002', 'item-2',  30),
        ( 3, '00000000-0000-0000-0000-000000000003', 'item-3',  70),
        ( 4, '00000000-0000-0000-0000-000000000004', 'item-4',  10),
        ( 5, '00000000-0000-0000-0000-000000000005', 'item-5',  90),
        ( 6, '00000000-0000-0000-0000-000000000006', 'item-6',  20),
        ( 7, '00000000-0000-0000-0000-000000000007', 'item-7',  60),
        ( 8, '00000000-0000-0000-0000-000000000008', 'item-8',  40),
        ( 9, '00000000-0000-0000-0000-000000000009', 'item-9',  80),
        (10, '00000000-0000-0000-0000-00000000000a', 'item-10', 15);
    SET IDENTITY_INSERT dbo.tb_relay_item OFF;
END
GO

-- ============================================================================
-- fraiseql_test: Scored items (window function + CTE + aggregation tests)
-- ============================================================================

IF OBJECT_ID('dbo.tb_score', 'U') IS NULL
    CREATE TABLE dbo.tb_score (
        id       NVARCHAR(36)  NOT NULL PRIMARY KEY,
        category NVARCHAR(50)  NOT NULL,
        score    INT           NOT NULL,
        label    NVARCHAR(100) NOT NULL
    );
GO

IF OBJECT_ID('dbo.v_score', 'V') IS NOT NULL
    DROP VIEW dbo.v_score;
GO

CREATE VIEW dbo.v_score AS
SELECT
    id,
    (SELECT id, category, score, label FOR JSON PATH, WITHOUT_ARRAY_WRAPPER) AS data
FROM dbo.tb_score;
GO

IF NOT EXISTS (SELECT 1 FROM dbo.tb_score WHERE id = 'sc-01')
BEGIN
    INSERT INTO dbo.tb_score (id, category, score, label) VALUES
        ('sc-01', 'A', 95, 'alpha'),
        ('sc-02', 'A', 80, 'beta'),
        ('sc-03', 'A', 80, 'gamma'),
        ('sc-04', 'B', 70, 'delta'),
        ('sc-05', 'B', 60, 'epsilon'),
        ('sc-06', 'B', 90, 'zeta'),
        ('sc-07', 'C', 50, 'eta'),
        ('sc-08', 'C', 55, 'theta');
END
GO

-- ============================================================================
-- fraiseql_test: Tags (mutation stored procedure tests)
-- ============================================================================

IF OBJECT_ID('dbo.tb_tag', 'U') IS NULL
    CREATE TABLE dbo.tb_tag (
        pk_tag INT           IDENTITY(1,1) PRIMARY KEY,
        name   NVARCHAR(200) NOT NULL UNIQUE
    );
GO

IF OBJECT_ID('dbo.v_tag', 'V') IS NOT NULL
    DROP VIEW dbo.v_tag;
GO

CREATE VIEW dbo.v_tag AS
SELECT pk_tag, name FROM dbo.tb_tag;
GO

IF OBJECT_ID('dbo.fn_create_tag', 'P') IS NOT NULL
    DROP PROCEDURE dbo.fn_create_tag;
GO

CREATE PROCEDURE dbo.fn_create_tag
    @p_name NVARCHAR(200)
AS
BEGIN
    SET NOCOUNT ON;
    MERGE dbo.tb_tag AS target
    USING (SELECT @p_name AS name) AS source
    ON target.name = source.name
    WHEN MATCHED THEN UPDATE SET name = source.name
    WHEN NOT MATCHED THEN INSERT (name) VALUES (source.name);

    SELECT pk_tag AS id, name FROM dbo.tb_tag WHERE name = @p_name;
END
GO
