-- FraiseQL Observer System - SQL Server NATS Transport Schema
-- This migration adds support for NATS JetStream transport on SQL Server:
-- - Transport checkpoint table for cursor persistence
-- - NATS publication tracking columns on entity change log
--
-- Note: SQL Server does not have LISTEN/NOTIFY, so the bridge uses polling only.
-- Compatible with SQL Server 2016+ and Azure SQL Database.

-- ============================================================================
-- Transport Checkpoint Table
-- ============================================================================
-- Stores cursor position for event transports, enabling crash recovery.
-- Each transport (e.g., "mssql_to_nats") maintains its own checkpoint.

IF NOT EXISTS (SELECT * FROM sys.objects WHERE object_id = OBJECT_ID(N'[dbo].[tb_transport_checkpoint]') AND type in (N'U'))
BEGIN
    CREATE TABLE [dbo].[tb_transport_checkpoint] (
        -- Transport identifier (e.g., "mssql_to_nats", "mssql_to_kafka")
        [transport_name] NVARCHAR(255) NOT NULL PRIMARY KEY,

        -- Last processed primary key from source table
        [last_pk] BIGINT NOT NULL,

        -- When the checkpoint was last updated
        [updated_at] DATETIME2 NOT NULL DEFAULT GETUTCDATE()
    );

    -- Add table description
    EXEC sp_addextendedproperty
        @name = N'MS_Description',
        @value = N'Stores cursor position for event transports (NATS bridge, etc.) for crash recovery',
        @level0type = N'SCHEMA', @level0name = N'dbo',
        @level1type = N'TABLE',  @level1name = N'tb_transport_checkpoint';
END;
GO

-- Index for monitoring/debugging queries (find stale checkpoints)
IF NOT EXISTS (SELECT * FROM sys.indexes WHERE name = 'idx_transport_checkpoint_updated_at' AND object_id = OBJECT_ID('tb_transport_checkpoint'))
BEGIN
    CREATE INDEX [idx_transport_checkpoint_updated_at]
    ON [dbo].[tb_transport_checkpoint]([updated_at] DESC);
END;
GO

-- ============================================================================
-- Entity Change Log Table
-- ============================================================================
-- Stores entity change events for NATS publishing.
-- This table follows the outbox pattern - application writes here,
-- bridge publishes to NATS.

IF NOT EXISTS (SELECT * FROM sys.objects WHERE object_id = OBJECT_ID(N'[dbo].[tb_entity_change_log]') AND type in (N'U'))
BEGIN
    CREATE TABLE [dbo].[tb_entity_change_log] (
        -- Primary key (used as cursor for bridge)
        [pk_entity_change_log] BIGINT IDENTITY(1,1) PRIMARY KEY,

        -- UUID identifier for the change
        [id] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),

        -- Customer organization ID (tenant)
        [fk_customer_org] BIGINT NULL,

        -- Contact ID (user who made the change)
        [fk_contact] BIGINT NULL,

        -- Entity type (e.g., "Order", "User")
        [object_type] NVARCHAR(255) NOT NULL,

        -- Entity ID (UUID of the changed entity)
        [object_id] UNIQUEIDENTIFIER NOT NULL,

        -- Modification type: INSERT, UPDATE, DELETE
        [modification_type] NVARCHAR(50) NOT NULL,

        -- Change status (e.g., "pending", "processed")
        [change_status] NVARCHAR(50) NULL,

        -- Entity data as JSON (SQL Server 2016+ supports JSON)
        [object_data] NVARCHAR(MAX) NULL,

        -- Extra metadata as JSON
        [extra_metadata] NVARCHAR(MAX) NULL,

        -- When the change was created
        [created_at] DATETIME2 NOT NULL DEFAULT GETUTCDATE(),

        -- When the change was published to NATS (NULL = not published)
        [nats_published_at] DATETIME2 NULL,

        -- NATS event ID (for deduplication)
        [nats_event_id] UNIQUEIDENTIFIER NULL,

        -- Constraint for modification type
        CONSTRAINT [CK_entity_change_log_modification_type]
            CHECK ([modification_type] IN ('INSERT', 'UPDATE', 'DELETE'))
    );

    -- Add table description
    EXEC sp_addextendedproperty
        @name = N'MS_Description',
        @value = N'Outbox table for entity change events, published to NATS by bridge process',
        @level0type = N'SCHEMA', @level0name = N'dbo',
        @level1type = N'TABLE',  @level1name = N'tb_entity_change_log';
END;
GO

-- Indexes for efficient querying
IF NOT EXISTS (SELECT * FROM sys.indexes WHERE name = 'idx_entity_change_log_object_type' AND object_id = OBJECT_ID('tb_entity_change_log'))
BEGIN
    CREATE INDEX [idx_entity_change_log_object_type]
    ON [dbo].[tb_entity_change_log]([object_type]);
END;
GO

IF NOT EXISTS (SELECT * FROM sys.indexes WHERE name = 'idx_entity_change_log_created_at' AND object_id = OBJECT_ID('tb_entity_change_log'))
BEGIN
    CREATE INDEX [idx_entity_change_log_created_at]
    ON [dbo].[tb_entity_change_log]([created_at]);
END;
GO

-- Index for bridge queries (cursor-based fetching)
IF NOT EXISTS (SELECT * FROM sys.indexes WHERE name = 'idx_entity_change_log_cursor' AND object_id = OBJECT_ID('tb_entity_change_log'))
BEGIN
    CREATE INDEX [idx_entity_change_log_cursor]
    ON [dbo].[tb_entity_change_log]([pk_entity_change_log])
    INCLUDE ([nats_published_at]);
END;
GO

-- ============================================================================
-- Add NATS columns if table already exists
-- ============================================================================
-- Use these ALTER statements if tb_entity_change_log already exists

-- IF NOT EXISTS (SELECT * FROM sys.columns WHERE object_id = OBJECT_ID('tb_entity_change_log') AND name = 'nats_published_at')
-- BEGIN
--     ALTER TABLE [dbo].[tb_entity_change_log]
--         ADD [nats_published_at] DATETIME2 NULL;
-- END;
-- GO

-- IF NOT EXISTS (SELECT * FROM sys.columns WHERE object_id = OBJECT_ID('tb_entity_change_log') AND name = 'nats_event_id')
-- BEGIN
--     ALTER TABLE [dbo].[tb_entity_change_log]
--         ADD [nats_event_id] UNIQUEIDENTIFIER NULL;
-- END;
-- GO

-- ============================================================================
-- Helper Stored Procedure: Insert Change Log Entry
-- ============================================================================
-- Call this from application code or triggers to record entity changes.

IF OBJECT_ID('sp_insert_entity_change', 'P') IS NOT NULL
    DROP PROCEDURE [dbo].[sp_insert_entity_change];
GO

CREATE PROCEDURE [dbo].[sp_insert_entity_change]
    @object_type NVARCHAR(255),
    @object_id UNIQUEIDENTIFIER,
    @modification_type NVARCHAR(50),
    @object_data NVARCHAR(MAX) = NULL,
    @fk_customer_org BIGINT = NULL,
    @fk_contact BIGINT = NULL
AS
BEGIN
    SET NOCOUNT ON;

    INSERT INTO [dbo].[tb_entity_change_log] (
        [fk_customer_org],
        [fk_contact],
        [object_type],
        [object_id],
        [modification_type],
        [object_data]
    ) VALUES (
        @fk_customer_org,
        @fk_contact,
        @object_type,
        @object_id,
        @modification_type,
        @object_data
    );
END;
GO

-- ============================================================================
-- Helper Stored Procedure: Upsert Checkpoint
-- ============================================================================
-- Uses MERGE for atomic upsert semantics.

IF OBJECT_ID('sp_upsert_checkpoint', 'P') IS NOT NULL
    DROP PROCEDURE [dbo].[sp_upsert_checkpoint];
GO

CREATE PROCEDURE [dbo].[sp_upsert_checkpoint]
    @transport_name NVARCHAR(255),
    @last_pk BIGINT
AS
BEGIN
    SET NOCOUNT ON;

    MERGE [dbo].[tb_transport_checkpoint] AS target
    USING (SELECT @transport_name AS transport_name, @last_pk AS last_pk) AS source
    ON (target.transport_name = source.transport_name)
    WHEN MATCHED THEN
        UPDATE SET
            last_pk = source.last_pk,
            updated_at = GETUTCDATE()
    WHEN NOT MATCHED THEN
        INSERT (transport_name, last_pk, updated_at)
        VALUES (source.transport_name, source.last_pk, GETUTCDATE());
END;
GO

-- ============================================================================
-- Example Trigger: Auto-capture Order changes
-- ============================================================================
-- Uncomment and modify for your entity tables.

-- IF OBJECT_ID('tr_orders_after_insert', 'TR') IS NOT NULL
--     DROP TRIGGER [dbo].[tr_orders_after_insert];
-- GO
--
-- CREATE TRIGGER [dbo].[tr_orders_after_insert]
-- ON [dbo].[orders]
-- AFTER INSERT
-- AS
-- BEGIN
--     SET NOCOUNT ON;
--
--     INSERT INTO [dbo].[tb_entity_change_log] (
--         fk_customer_org, fk_contact, object_type, object_id, modification_type, object_data
--     )
--     SELECT
--         i.customer_org_id,
--         i.created_by,
--         'Order',
--         i.id,
--         'INSERT',
--         (SELECT i.id, i.total, i.status FOR JSON PATH, WITHOUT_ARRAY_WRAPPER)
--     FROM inserted i;
-- END;
-- GO
--
-- IF OBJECT_ID('tr_orders_after_update', 'TR') IS NOT NULL
--     DROP TRIGGER [dbo].[tr_orders_after_update];
-- GO
--
-- CREATE TRIGGER [dbo].[tr_orders_after_update]
-- ON [dbo].[orders]
-- AFTER UPDATE
-- AS
-- BEGIN
--     SET NOCOUNT ON;
--
--     INSERT INTO [dbo].[tb_entity_change_log] (
--         fk_customer_org, fk_contact, object_type, object_id, modification_type, object_data
--     )
--     SELECT
--         i.customer_org_id,
--         i.updated_by,
--         'Order',
--         i.id,
--         'UPDATE',
--         (SELECT i.id, i.total, i.status FOR JSON PATH, WITHOUT_ARRAY_WRAPPER)
--     FROM inserted i;
-- END;
-- GO
--
-- IF OBJECT_ID('tr_orders_after_delete', 'TR') IS NOT NULL
--     DROP TRIGGER [dbo].[tr_orders_after_delete];
-- GO
--
-- CREATE TRIGGER [dbo].[tr_orders_after_delete]
-- ON [dbo].[orders]
-- AFTER DELETE
-- AS
-- BEGIN
--     SET NOCOUNT ON;
--
--     INSERT INTO [dbo].[tb_entity_change_log] (
--         fk_customer_org, fk_contact, object_type, object_id, modification_type, object_data
--     )
--     SELECT
--         d.customer_org_id,
--         NULL,
--         'Order',
--         d.id,
--         'DELETE',
--         (SELECT d.id FOR JSON PATH, WITHOUT_ARRAY_WRAPPER)
--     FROM deleted d;
-- END;
-- GO

-- ============================================================================
-- Monitoring View
-- ============================================================================
-- View for monitoring NATS publication status and lag

IF OBJECT_ID('vw_nats_publication_status', 'V') IS NOT NULL
    DROP VIEW [dbo].[vw_nats_publication_status];
GO

CREATE VIEW [dbo].[vw_nats_publication_status] AS
SELECT
    checkpoint.transport_name,
    checkpoint.last_pk AS checkpoint_cursor,
    checkpoint.updated_at AS checkpoint_updated_at,
    (SELECT MAX(pk_entity_change_log) FROM tb_entity_change_log) AS max_pk,
    (SELECT MAX(pk_entity_change_log) FROM tb_entity_change_log) - checkpoint.last_pk AS lag_count,
    (SELECT COUNT(*) FROM tb_entity_change_log WHERE nats_published_at IS NULL) AS unpublished_count
FROM tb_transport_checkpoint checkpoint
WHERE checkpoint.transport_name LIKE 'mssql_to_nats%';
GO
