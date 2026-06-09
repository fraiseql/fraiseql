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
    updated_fields       JSON         NULL,
    `cascade`            JSON         NULL,
    -- Perf observability (#392): NULL on MySQL (no request-scoped GUC clock).
    duration_ms          INT          NULL,
    started_at           TIMESTAMP(6) NULL,
    -- Durable ordering / dedup (#382). seq: producer-supplied (no native SEQUENCE).
    commit_time          TIMESTAMP(6) NULL,
    seq                  BIGINT       NULL,
    -- Actor model (#390): columns now, values when #390 lands.
    actor_type           VARCHAR(50)  NULL,
    acting_for           BIGINT       NULL,
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

ALTER TABLE tb_entity_change_log
    COMMENT = 'FraiseQL change-log contract (Change Spine Tier 0 outbox), MySQL variant. Superset of perf (#392) + envelope columns. See docs/architecture/change-log-contract.md.';
