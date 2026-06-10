-- FraiseQL Change-Log Contract — SQL Server variant of core.tb_entity_change_log
-- ============================================================================
-- SQL Server dialect of the framework-owned change-log contract (the superset
-- table defined for PostgreSQL in 08_create_entity_change_log_contract.sql).
-- The Change Spine outbox is a plain INSERT, so it is portable across dialects;
-- this file carries the SQL Server types for the same contract columns.
--
-- Run this in the target database, e.g.:  sqlcmd -d mydb -i this_file.sql
--
-- Trinity types: tenant_id is the public-facing UUID (UNIQUEIDENTIFIER);
-- fk_customer_org is the internal BIGINT join FK — complementary, never collapsed.
--
-- Dialect notes:
--   * SQL Server HAS native sequences, so `seq` defaults to NEXT VALUE FOR a
--     CREATE SEQUENCE — the monotonic source any INSERTer (incl. cooperative
--     external producers) gets, matching PostgreSQL's core.seq_entity_change_log.
--   * TEXT[]/JSONB become NVARCHAR(MAX) (JSON text); TIMESTAMPTZ becomes
--     DATETIME2; UUID becomes UNIQUEIDENTIFIER.

IF NOT EXISTS (SELECT 1 FROM sys.schemas WHERE name = 'core')
    EXEC('CREATE SCHEMA core');
GO

IF NOT EXISTS (SELECT 1 FROM sys.sequences WHERE name = 'seq_entity_change_log'
               AND schema_id = SCHEMA_ID('core'))
    CREATE SEQUENCE core.seq_entity_change_log AS BIGINT START WITH 1 INCREMENT BY 1;
GO

IF OBJECT_ID('core.tb_entity_change_log', 'U') IS NULL
CREATE TABLE core.tb_entity_change_log (
    -- Backbone.
    pk_entity_change_log BIGINT IDENTITY(1,1) PRIMARY KEY,
    object_type          NVARCHAR(255) NOT NULL,
    modification_type    NVARCHAR(50)  NOT NULL,
    id                   UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),
    created_at           DATETIME2     NOT NULL DEFAULT SYSUTCDATETIME(),
    -- Spine envelope: RLS/JWT partition stamp (public-facing UUID).
    tenant_id            UNIQUEIDENTIFIER NULL,
    -- Internal join FKs (Trinity fk_{entity}) — kept alongside tenant_id.
    fk_customer_org      BIGINT        NULL,
    fk_contact           BIGINT        NULL,
    -- Changed-entity identity + payload (JSON as NVARCHAR(MAX)).
    object_id            UNIQUEIDENTIFIER NULL,
    object_data          NVARCHAR(MAX) NULL,
    updated_fields       NVARCHAR(MAX) NULL,
    -- `cascade` is a reserved keyword in SQL Server → bracket-quoted (an unquoted
    -- `cascade` is a syntax error). The portable outbox INSERT quotes it likewise.
    [cascade]            NVARCHAR(MAX) NULL,
    -- Perf observability (#392): NULL on SQL Server (no request-scoped GUC clock).
    duration_ms          INT           NULL,
    started_at           DATETIME2     NULL,
    -- Durable ordering / dedup (#382). seq: native sequence default below.
    commit_time          DATETIME2     NULL,
    seq                  BIGINT        NOT NULL DEFAULT (NEXT VALUE FOR core.seq_entity_change_log),
    -- Actor model (#390): the request's actor classification + the delegated
    -- human a delegated agent acts for (public-facing UUID, UNIQUEIDENTIFIER).
    actor_type           NVARCHAR(50)  NULL,
    acting_for           UNIQUEIDENTIFIER NULL,
    -- Replay correctness (#377/#378): column now, value when #377 lands.
    schema_version       NVARCHAR(255) NULL,
    -- W3C trace context (#375): columns now, values when #375 lands.
    trace_id             NVARCHAR(255) NULL,
    trace_context        NVARCHAR(MAX) NULL,
    -- Existing reader columns (poller / NATS bridge).
    change_status        NVARCHAR(50)  NULL,
    extra_metadata       NVARCHAR(MAX) NULL,
    nats_published_at    DATETIME2     NULL,
    nats_event_id        UNIQUEIDENTIFIER NULL
);
GO

-- Retype `acting_for` BIGINT -> UNIQUEIDENTIFIER (#390) on a table that ran the
-- v2.6.0 BIGINT form (the CREATE above is skipped when the table exists). Done as
-- DROP + ADD because the column has always been NULL (lossless) and SQL Server
-- has no bigint -> uniqueidentifier conversion for ALTER COLUMN. Guarded on the
-- live column type, so it is a no-op on a fresh (already-UNIQUEIDENTIFIER) table.
IF EXISTS (
    SELECT 1 FROM sys.columns c
    JOIN sys.types t ON c.user_type_id = t.user_type_id
    WHERE c.object_id = OBJECT_ID('core.tb_entity_change_log')
      AND c.name = 'acting_for'
      AND t.name <> 'uniqueidentifier'
)
BEGIN
    ALTER TABLE core.tb_entity_change_log DROP COLUMN acting_for;
    ALTER TABLE core.tb_entity_change_log ADD acting_for UNIQUEIDENTIFIER NULL;
END
GO

IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = 'idx_entity_log_type'
               AND object_id = OBJECT_ID('core.tb_entity_change_log'))
    CREATE INDEX idx_entity_log_type ON core.tb_entity_change_log (object_type);
GO
IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = 'idx_entity_log_created'
               AND object_id = OBJECT_ID('core.tb_entity_change_log'))
    CREATE INDEX idx_entity_log_created ON core.tb_entity_change_log (created_at);
GO
IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = 'idx_entity_log_tenant_seq'
               AND object_id = OBJECT_ID('core.tb_entity_change_log'))
    CREATE INDEX idx_entity_log_tenant_seq ON core.tb_entity_change_log (tenant_id, seq);
GO
IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = 'idx_entity_log_type_seq'
               AND object_id = OBJECT_ID('core.tb_entity_change_log'))
    CREATE INDEX idx_entity_log_type_seq ON core.tb_entity_change_log (object_type, seq);
GO
