-- FraiseQL Change-Log Contract — MySQL variant of core.tb_entity_change_log
-- ============================================================================
-- MySQL dialect of the framework-owned change-log contract (the superset table
-- defined for PostgreSQL in 08_create_entity_change_log_contract.sql). The
-- Change Spine outbox is a plain INSERT, so it is portable across dialects;
-- this file carries the MySQL types for the same contract columns.
--
-- MySQL has no schemas (it uses databases), so the table is unqualified — run
-- this in the target database:  mysql -u root mydb < this_file.sql
--
-- Trinity types: tenant_id is the public-facing UUID (CHAR(36)); fk_customer_org
-- is the internal BIGINT join FK — complementary, never collapsed.
--
-- Dialect notes:
--   * MySQL has no native SEQUENCE, so `seq` is a plain BIGINT supplied by the
--     producer (or left NULL). The AUTO_INCREMENT pk provides physical ordering;
--     a trigger-assigned monotonic `seq` is a follow-up (mirrors the PostgreSQL
--     core.seq_entity_change_log).
--   * TEXT[]/JSONB become JSON; TIMESTAMPTZ becomes TIMESTAMP(6) (UTC by
--     convention); UUID becomes CHAR(36).

CREATE TABLE IF NOT EXISTS tb_entity_change_log (
    -- Backbone.
    pk_entity_change_log BIGINT AUTO_INCREMENT PRIMARY KEY,
    object_type          VARCHAR(255) NOT NULL,
    modification_type    VARCHAR(50)  NOT NULL,
    -- DEFAULT (UUID()) so the portable outbox INSERT (which omits `id`, exactly
    -- as on PG/MSSQL) is well-formed; MySQL 8.0.13+ allows expression defaults.
    id                   CHAR(36)     NOT NULL DEFAULT (UUID()),
    created_at           TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    -- Spine envelope: RLS/JWT partition stamp (public-facing UUID).
    tenant_id            CHAR(36)     NULL,
    -- Internal join FKs (Trinity fk_{entity}) — kept alongside tenant_id.
    fk_customer_org      BIGINT       NULL,
    fk_contact           BIGINT       NULL,
    -- Changed-entity identity + payload.
    object_id            CHAR(36)     NULL,
    object_data          JSON         NULL,
    -- Opt-in pre-image (changelog_pre_image): the changed entity's BEFORE-state.
    -- object_data stays the after-image; the pre-image is this separate column,
    -- never an envelope. Contract parity — the Change Spine outbox CTE that
    -- populates it is PostgreSQL-only; NULL on MySQL until a writer lands.
    object_data_before   JSON         NULL,
    updated_fields       JSON         NULL,
    `cascade`            JSON         NULL,
    -- Perf observability (#392): NULL on MySQL (no request-scoped GUC clock).
    duration_ms          INT          NULL,
    started_at           TIMESTAMP(6) NULL,
    -- Durable ordering / dedup (#382). seq: producer-supplied (no native SEQUENCE).
    commit_time          TIMESTAMP(6) NULL,
    seq                  BIGINT       NULL,
    -- Actor model (#390): the request's actor classification + the delegated
    -- human a delegated agent acts for (public-facing UUID, CHAR(36)).
    actor_type           VARCHAR(50)  NULL,
    acting_for           CHAR(36)     NULL,
    -- Replay correctness (#377/#378): column now, value when #377 lands.
    schema_version       VARCHAR(255) NULL,
    -- W3C trace context (#375): columns now, values when #375 lands.
    trace_id             VARCHAR(255) NULL,
    trace_context        JSON         NULL,
    -- Existing reader columns (poller / NATS bridge).
    change_status        VARCHAR(50)  NULL,
    extra_metadata       JSON         NULL,
    nats_published_at    TIMESTAMP(6) NULL,
    nats_event_id        CHAR(36)     NULL,
    -- Indexes mirroring the PostgreSQL contract.
    INDEX idx_entity_log_type (object_type),
    INDEX idx_entity_log_created (created_at),
    INDEX idx_entity_log_tenant_seq (tenant_id, seq),
    INDEX idx_entity_log_type_seq (object_type, seq)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Retype `acting_for` BIGINT -> CHAR(36) (#390) on a table that ran the v2.6.0
-- BIGINT form (CREATE TABLE IF NOT EXISTS above is a no-op there). Lossless: the
-- column has always been NULL. Guarded on the live column type via a prepared
-- statement (MySQL has no anonymous DO block / conditional ALTER); a fresh table
-- is already CHAR(36), so the guard yields a no-op `SELECT 1`.
SET @retype_acting_for := (
    SELECT IF(DATA_TYPE = 'char',
              'SELECT 1',
              'ALTER TABLE tb_entity_change_log MODIFY COLUMN acting_for CHAR(36) NULL')
    FROM information_schema.COLUMNS
    WHERE TABLE_SCHEMA = DATABASE()
      AND TABLE_NAME = 'tb_entity_change_log'
      AND COLUMN_NAME = 'acting_for'
);
PREPARE retype_stmt FROM @retype_acting_for;
EXECUTE retype_stmt;
DEALLOCATE PREPARE retype_stmt;

-- Add `object_data_before` (changelog_pre_image) on a pre-existing table (the
-- CREATE TABLE IF NOT EXISTS above is a no-op there). MySQL 8.0 has no
-- `ADD COLUMN IF NOT EXISTS`, so guard on information_schema via a prepared
-- statement (same shape as the acting_for retype); a fresh table already has the
-- column, so the guard yields a no-op `SELECT 1`.
SET @add_object_data_before := (
    SELECT IF(COUNT(*) > 0,
              'SELECT 1',
              'ALTER TABLE tb_entity_change_log ADD COLUMN object_data_before JSON NULL AFTER object_data')
    FROM information_schema.COLUMNS
    WHERE TABLE_SCHEMA = DATABASE()
      AND TABLE_NAME = 'tb_entity_change_log'
      AND COLUMN_NAME = 'object_data_before'
);
PREPARE add_odb_stmt FROM @add_object_data_before;
EXECUTE add_odb_stmt;
DEALLOCATE PREPARE add_odb_stmt;

ALTER TABLE tb_entity_change_log
    COMMENT = 'FraiseQL change-log contract (Change Spine Tier 0 outbox), MySQL variant. Superset of perf (#392) + envelope columns. See docs/architecture/change-log-contract.md.';
