-- FraiseQL Change-Log Contract — core.tb_entity_change_log
-- ============================================================================
-- The framework-owned, superset-column definition of the entity change-log
-- table: the first step of the "Change Spine" (an app-mediated transactional
-- outbox the mutation executor writes in-txn; see
-- docs/architecture/change-log-contract.md).
--
-- This migration OWNS the table contract. It supersedes the best-effort
-- `core.v_entity_change_log` view that 07_create_changelog_views.sql used to
-- create (07 now ships only the transport-checkpoint view + upsert fn).
--
-- It is **purely additive and idempotent**:
--   - CREATE TABLE IF NOT EXISTS installs the NOT-NULL backbone for a fresh DB.
--   - ALTER ... ADD COLUMN IF NOT EXISTS brings a pre-existing (app-created /
--     older-framework) table up to the contract — every added column is
--     nullable or defaulted, so it is safe on a populated table.
--   - It never drops or renames an existing column. In particular `tenant_id`
--     (the RLS/JWT partition stamp) is ADDED alongside `fk_customer_org` (the
--     internal join FK) — the two are complementary under the Trinity pattern,
--     NOT a rename.
--
-- PostgreSQL DDL. MySQL / SQL Server variants are 09_*/10_* (mirroring
-- migrations 04_*/05_*). The view is the portable read surface.

CREATE SCHEMA IF NOT EXISTS core;

-- ----------------------------------------------------------------------------
-- Backbone (NOT NULL, no default) — created only on a fresh table. Every
-- pre-existing change-log table inherently carries object_type/modification_type,
-- so CREATE IF NOT EXISTS no-ops and we never add a no-default NOT NULL column
-- to live rows.
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS core.tb_entity_change_log (
    pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    object_type          TEXT NOT NULL,
    modification_type    TEXT NOT NULL
);

-- ----------------------------------------------------------------------------
-- Contract columns (nullable, or NOT NULL with a safe default) — additive
-- reconcile path. Native ADD COLUMN IF NOT EXISTS makes each statement
-- idempotent and safe on a table with live rows: id/created_at backfill from
-- their defaults, every other column is nullable.
-- ----------------------------------------------------------------------------
ALTER TABLE core.tb_entity_change_log
    -- Defaulted backbone (backfills on a pre-existing table that lacks them).
    ADD COLUMN IF NOT EXISTS id                 UUID        NOT NULL DEFAULT gen_random_uuid(),
    ADD COLUMN IF NOT EXISTS created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Spine envelope: RLS/JWT partition stamp, the Trinity public-facing
    -- identifier (UUID), complementary to the internal join FK fk_customer_org
    -- (BIGINT). Stamped explicitly from SecurityContext.tenant_id at write time.
    ADD COLUMN IF NOT EXISTS tenant_id          UUID,
    -- Internal join FKs (Trinity fk_{entity}) — existing observer schema, kept.
    ADD COLUMN IF NOT EXISTS fk_customer_org    BIGINT,
    ADD COLUMN IF NOT EXISTS fk_contact         BIGINT,
    -- Changed-entity identity + payload.
    ADD COLUMN IF NOT EXISTS object_id          UUID,
    ADD COLUMN IF NOT EXISTS object_data        JSONB,
    -- Opt-in pre-image (changelog_pre_image): the changed entity's BEFORE-state,
    -- recorded only by mutations/tables that opt in (NULL otherwise). object_data
    -- stays the AFTER-image from EVERY producer (executor outbox AND #366 capture
    -- trigger); the pre-image lives in this separate column, never as a
    -- {before,after} envelope inside object_data. A Debezium event is the
    -- core.v_entity_change_log_debezium projection below, not a stored shape.
    ADD COLUMN IF NOT EXISTS object_data_before JSONB,
    ADD COLUMN IF NOT EXISTS updated_fields     TEXT[],
    ADD COLUMN IF NOT EXISTS cascade            JSONB,
    -- Perf observability (#392): populated by the executor's in-txn write.
    ADD COLUMN IF NOT EXISTS duration_ms        INTEGER,
    ADD COLUMN IF NOT EXISTS started_at         TIMESTAMPTZ,
    -- Durable ordering / dedup (#382 broker fan-out).
    ADD COLUMN IF NOT EXISTS commit_time        TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS seq                BIGINT,
    -- Actor model (#390): the request's actor classification + the delegated
    -- human a delegated agent acts for. `acting_for` is the Trinity public-facing
    -- UUID (like tenant_id), so the executor stamps it without a DB lookup.
    ADD COLUMN IF NOT EXISTS actor_type         TEXT,
    ADD COLUMN IF NOT EXISTS acting_for         UUID,
    -- Replay correctness (#377/#378): column now, value when #377 lands.
    ADD COLUMN IF NOT EXISTS schema_version     TEXT,
    -- W3C trace context (#375): columns now, values when #375 lands.
    ADD COLUMN IF NOT EXISTS trace_id           TEXT,
    ADD COLUMN IF NOT EXISTS trace_context      JSONB,
    -- Existing reader columns (poller / NATS bridge).
    ADD COLUMN IF NOT EXISTS change_status      TEXT,
    ADD COLUMN IF NOT EXISTS extra_metadata     JSONB,
    ADD COLUMN IF NOT EXISTS nats_published_at  TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS nats_event_id      UUID;

-- ----------------------------------------------------------------------------
-- Retype `acting_for` BIGINT -> UUID (#390). v2.6.0 shipped this column as
-- BIGINT (NULL-by-design, no producer), so on a DB that already ran that form
-- the ADD COLUMN IF NOT EXISTS above is a no-op and leaves it BIGINT. This
-- guarded ALTER brings it to the contract UUID type. `USING NULL` is the only
-- valid conversion (int8 -> uuid has no cast) and is LOSSLESS here precisely
-- because the column has always been NULL — no #390-era row exists yet. Guarded
-- on udt_name so it is a no-op on a fresh (already-UUID) table and re-run safe.
-- ----------------------------------------------------------------------------
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'core'
          AND table_name = 'tb_entity_change_log'
          AND column_name = 'acting_for'
          AND udt_name <> 'uuid'
    ) THEN
        ALTER TABLE core.tb_entity_change_log
            ALTER COLUMN acting_for TYPE UUID USING NULL;
    END IF;
END $$;

-- ----------------------------------------------------------------------------
-- Monotonic `seq` source (Change Spine durable ordering / dedup on
-- (object_type, seq)). A plain global SEQUENCE defaulted on the column, so ANY
-- INSERTer gets a value — the FraiseQL executor AND cooperative external
-- producers writing contract-conforming rows (the seq is not an executor-only
-- counter). Re-run safe: CREATE SEQUENCE IF NOT EXISTS + idempotent SET DEFAULT.
-- ----------------------------------------------------------------------------
CREATE SEQUENCE IF NOT EXISTS core.seq_entity_change_log;
ALTER TABLE core.tb_entity_change_log
    ALTER COLUMN seq SET DEFAULT nextval('core.seq_entity_change_log');
ALTER SEQUENCE core.seq_entity_change_log OWNED BY core.tb_entity_change_log.seq;

-- ----------------------------------------------------------------------------
-- Indexes (re-run safe).
-- ----------------------------------------------------------------------------
-- Slowest-mutation ordering (#392 perf forensics).
CREATE INDEX IF NOT EXISTS idx_entity_log_duration
    ON core.tb_entity_change_log (duration_ms DESC);
-- Per-object-type scans (perf + reader).
CREATE INDEX IF NOT EXISTS idx_entity_log_type
    ON core.tb_entity_change_log (object_type);
-- Time-range scans.
CREATE INDEX IF NOT EXISTS idx_entity_log_created
    ON core.tb_entity_change_log (created_at);
-- Per-tenant ordered fan-out (RLS-clean consumer reads).
CREATE INDEX IF NOT EXISTS idx_entity_log_tenant_seq
    ON core.tb_entity_change_log (tenant_id, seq);
-- Per-object-type dedup on (object_type, seq).
CREATE INDEX IF NOT EXISTS idx_entity_log_type_seq
    ON core.tb_entity_change_log (object_type, seq);

-- ----------------------------------------------------------------------------
-- Read-path view (#392 consumes duration_ms; #149 consumes the `data` JSONB).
-- Superset of the legacy 07 view: keeps every #149 GraphQL `data` key (so the
-- entity_change_logs query stays stable) and adds the perf/envelope columns as
-- top-level columns for indexed WHERE/ORDER BY.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE VIEW core.v_entity_change_log AS
SELECT
    pk_entity_change_log,
    object_type,
    modification_type,
    object_id,
    tenant_id,
    duration_ms,
    started_at,
    trace_id,
    actor_type,
    acting_for,
    seq,
    created_at,
    jsonb_build_object(
        'id',                   id,
        'pk_entity_change_log', pk_entity_change_log,
        'tenant_id',            tenant_id,
        'fk_customer_org',      fk_customer_org,
        'fk_contact',           fk_contact,
        'object_type',          object_type,
        'object_id',            object_id,
        'modification_type',    modification_type,
        'change_status',        change_status,
        'object_data',          object_data,
        'object_data_before',   object_data_before,
        'updated_fields',       updated_fields,
        'cascade',              cascade,
        'duration_ms',          duration_ms,
        'started_at',           started_at,
        'actor_type',           actor_type,
        'acting_for',           acting_for,
        'extra_metadata',       extra_metadata,
        'created_at',           created_at
    ) AS data
FROM core.tb_entity_change_log;

COMMENT ON TABLE core.tb_entity_change_log IS
    'FraiseQL change-log contract (Change Spine Tier 0 outbox). Owned by the framework; superset of perf (#392) + envelope columns. See docs/architecture/change-log-contract.md.';

COMMENT ON VIEW core.v_entity_change_log IS
    'Read projection over tb_entity_change_log. Exposes duration_ms + envelope columns top-level (perf #392) and every GraphQL field in the data JSONB (#149). Cursor key: pk_entity_change_log.';

-- ----------------------------------------------------------------------------
-- Debezium projection — a view, NOT a stored shape (changelog_pre_image).
-- The base table keeps object_data uniformly the after-image and the pre-image
-- in object_data_before; `op`/`source` are already columns, so the classic
-- Debezium `{before, after, op, source}` event is a pure projection. Consumers
-- that want a Debezium-shaped event read this view; the base table never stores
-- an envelope. `before` is NULL for rows whose producer did not opt into the
-- pre-image; `after` is NULL for a DELETE (the row has no after-state).
-- ----------------------------------------------------------------------------
CREATE OR REPLACE VIEW core.v_entity_change_log_debezium AS
SELECT
    pk_entity_change_log,
    seq,
    object_data_before AS before,
    object_data        AS after,
    modification_type  AS op,            -- INSERT / UPDATE / DELETE / CUSTOM
    jsonb_build_object(
        'object_type', object_type,
        'object_id',   object_id,
        'tenant_id',   tenant_id,
        'commit_time', commit_time,
        'seq',         seq,
        'trace_id',    trace_id,
        'actor_type',  actor_type
    ) AS source
FROM core.tb_entity_change_log;

COMMENT ON VIEW core.v_entity_change_log_debezium IS
    'Debezium {before, after, op, source} projection over tb_entity_change_log (changelog_pre_image). before = object_data_before, after = object_data (the uniform after-image), op = modification_type. Pure projection — no envelope is stored in the base table.';
