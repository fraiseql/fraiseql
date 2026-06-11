-- FraiseQL External-Write Capture Trigger — core.fn_entity_change_log_capture
-- ============================================================================
-- The shipped, suppressible fallback-capture trigger function for #366. It
-- brings *uncooperative external writes* — a raw `INSERT INTO tb_post` from psql,
-- a migration, a background job, or a third-party tool — onto the Change Spine,
-- WITHOUT double-emitting for writes that already flow through FraiseQL's
-- mutation executor (which writes its own in-transaction outbox row).
--
-- Suppression contract
-- --------------------
-- The mutation executor sets the transaction-local GUC
-- `fraiseql.cdc_mediated = 'on'` (fraiseql_db::CDC_MEDIATED_VAR /
-- CDC_MEDIATED_ON) at the start of every mutation transaction. This function
-- checks it FIRST and returns without writing when it is set — so an app-path
-- write, already logged by the executor outbox, is never captured twice. A raw
-- external write leaves the GUC unset (`current_setting(..., true)` → NULL ≠
-- 'on'), so the trigger fires and captures the change. Exactly one change-log
-- row per write, on both paths.
--
-- Statement-level + transition tables (bulk-efficient)
-- ----------------------------------------------------
-- The triggers are AFTER ... FOR EACH STATEMENT and reference PostgreSQL
-- transition tables (`old_table` / `new_table`), so a single bulk statement
-- (`UPDATE tb_post SET ... WHERE ...` touching 1M rows) captures all its rows in
-- ONE set-based `INSERT ... SELECT`, not 1M per-row trigger invocations — while
-- still producing one change-log row (one event) per changed row, the correct
-- CDC granularity. The transition-table names are fixed by the generated
-- `CREATE TRIGGER ... REFERENCING` clauses (see the `fraiseql generate
-- capture-triggers` DDL generator), and TG_OP selects which one exists.
--
-- Contract-conforming rows
-- ------------------------
-- Each captured row is a first-class `core.tb_entity_change_log` row (see
-- docs/architecture/change-log-contract.md):
--   * `object_type`       = TG_ARGV[0], the GraphQL type name (e.g. 'Post') — the
--                           same value the executor stamps, so the existing
--                           reader/poller and the NATS bridges fan it out
--                           unchanged (subscription matching keys on the type
--                           name, never the table name).
--   * `modification_type` = TG_OP ('INSERT' | 'UPDATE' | 'DELETE').
--   * `object_id`         = the row's PK column (TG_ARGV[1], default 'id'),
--                           which MUST be a UUID — see the guard below.
--   * `object_data`       = a Debezium-style envelope `{op, before, after}` with
--                           the lowercase op code ('c' | 'u' | 'd'); this is the
--                           exact shape the reader decodes
--                           (`ChangeLogEntry::debezium_operation` /
--                           `after_values` / `before_values`).
--   * `tenant_id`         = the configured tenant column (TG_ARGV[2], default
--                           'tenant_id') if present and UUID-shaped, else the
--                           cooperative session GUC `fraiseql.tenant_id` — so the
--                           existing per-tenant subscription filtering applies.
--   * cooperative envelope = `actor_type` / `acting_for` / `schema_version` are
--                           stamped from the matching `fraiseql.*` session GUCs
--                           when a cooperative external writer sets them, else
--                           NULL (degraded but valid).
--   * `extra_metadata`    = `{"cdc_source": "fallback_trigger"}` so a captured row
--                           is distinguishable from an executor-written one.
--   * `seq` / `id` / `created_at` fire from the table's own column defaults.
--
-- UUID-PK guard (poller-stall safety)
-- -----------------------------------
-- The change-log reader decodes `object_id` as a NON-NULL `uuid` over the whole
-- batch (`sqlx::FromRow` / `fetch_all`), so a single row with a NULL/!UUID
-- `object_id` would fail the decode for the ENTIRE batch and permanently stall
-- the poller. The capture `SELECT` therefore filters to rows whose PK is a valid
-- UUID (`~ uuid_regex`) and casts only those — a misconfigured `@subscribable`
-- table (no UUID `id` column) silently captures nothing rather than stalling all
-- subscribers. `@subscribable` tables MUST expose a UUID public id column.
--
-- session_replication_role caveat
-- -------------------------------
-- These are ordinary (origin) triggers, so a session running with
-- `session_replication_role = replica` (logical-replication apply, some bulk
-- loaders) does NOT fire them — such writes are not captured. This is the
-- documented opt-out for true bulk loads.
--
-- PostgreSQL only (the suppression GUC is transaction-local PG state). The
-- function is idempotent (`CREATE OR REPLACE`).

CREATE SCHEMA IF NOT EXISTS core;

CREATE OR REPLACE FUNCTION core.fn_entity_change_log_capture()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
DECLARE
    -- Install-time configuration, baked into each CREATE TRIGGER via TG_ARGV.
    v_object_type TEXT := TG_ARGV[0];                       -- GraphQL type name
    v_pk_col      TEXT := COALESCE(TG_ARGV[1], 'id');       -- UUID public-id column
    v_tenant_col  TEXT := COALESCE(TG_ARGV[2], 'tenant_id');-- tenant column ('' = none)
    -- Cooperative session enrichment (NULL for an anonymous external write).
    v_actor_type     TEXT := NULLIF(current_setting('fraiseql.actor_type',     true), '');
    v_schema_version TEXT := NULLIF(current_setting('fraiseql.schema_version', true), '');
    v_acting_for_txt TEXT := NULLIF(current_setting('fraiseql.acting_for',     true), '');
    v_tenant_guc_txt TEXT := NULLIF(current_setting('fraiseql.tenant_id',      true), '');
    v_acting_for     UUID := NULL;
    v_tenant_guc     UUID := NULL;
    -- A strict UUID shape so a bad PK/tenant value never aborts the user's write
    -- (an unguarded `::uuid` cast inside the INSERT would raise in their txn).
    c_uuid_re CONSTANT TEXT :=
        '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$';
BEGIN
    -- (1) App-path writes are already logged by the executor outbox → suppress.
    IF current_setting('fraiseql.cdc_mediated', true) = 'on' THEN
        RETURN NULL;
    END IF;

    -- Pre-parse the cooperative GUCs once (guarded — never abort the user's txn).
    IF v_acting_for_txt ~ c_uuid_re THEN
        v_acting_for := v_acting_for_txt::uuid;
    END IF;
    IF v_tenant_guc_txt ~ c_uuid_re THEN
        v_tenant_guc := v_tenant_guc_txt::uuid;
    END IF;

    -- (2) Capture, set-based, per TG_OP. Each branch reads only the transition
    --     table that exists for that operation. The WHERE clause enforces the
    --     UUID-PK guard so a NULL/!UUID object_id can never reach the poller.
    IF TG_OP = 'INSERT' THEN
        INSERT INTO core.tb_entity_change_log
            (object_type, modification_type, object_id, object_data, tenant_id,
             actor_type, acting_for, schema_version, change_status, extra_metadata,
             commit_time)
        SELECT
            v_object_type,
            'INSERT',
            (to_jsonb(n) ->> v_pk_col)::uuid,
            jsonb_build_object('op', 'c', 'before', NULL, 'after', to_jsonb(n)),
            COALESCE(
                CASE WHEN (to_jsonb(n) ->> v_tenant_col) ~ c_uuid_re
                     THEN (to_jsonb(n) ->> v_tenant_col)::uuid END,
                v_tenant_guc),
            v_actor_type, v_acting_for, v_schema_version,
            'success',
            jsonb_build_object('cdc_source', 'fallback_trigger'),
            clock_timestamp()
        FROM new_table n
        WHERE (to_jsonb(n) ->> v_pk_col) ~ c_uuid_re;

    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO core.tb_entity_change_log
            (object_type, modification_type, object_id, object_data, tenant_id,
             actor_type, acting_for, schema_version, change_status, extra_metadata,
             commit_time)
        SELECT
            v_object_type,
            'UPDATE',
            (to_jsonb(n) ->> v_pk_col)::uuid,
            jsonb_build_object('op', 'u', 'before', to_jsonb(o), 'after', to_jsonb(n)),
            COALESCE(
                CASE WHEN (to_jsonb(n) ->> v_tenant_col) ~ c_uuid_re
                     THEN (to_jsonb(n) ->> v_tenant_col)::uuid END,
                v_tenant_guc),
            v_actor_type, v_acting_for, v_schema_version,
            'success',
            jsonb_build_object('cdc_source', 'fallback_trigger'),
            clock_timestamp()
        -- Pair OLD and NEW rows on the PK (transition tables are unordered sets).
        FROM new_table n
        JOIN old_table o ON (to_jsonb(o) ->> v_pk_col) = (to_jsonb(n) ->> v_pk_col)
        WHERE (to_jsonb(n) ->> v_pk_col) ~ c_uuid_re;

    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO core.tb_entity_change_log
            (object_type, modification_type, object_id, object_data, tenant_id,
             actor_type, acting_for, schema_version, change_status, extra_metadata,
             commit_time)
        SELECT
            v_object_type,
            'DELETE',
            (to_jsonb(o) ->> v_pk_col)::uuid,
            jsonb_build_object('op', 'd', 'before', to_jsonb(o), 'after', NULL),
            COALESCE(
                CASE WHEN (to_jsonb(o) ->> v_tenant_col) ~ c_uuid_re
                     THEN (to_jsonb(o) ->> v_tenant_col)::uuid END,
                v_tenant_guc),
            v_actor_type, v_acting_for, v_schema_version,
            'success',
            jsonb_build_object('cdc_source', 'fallback_trigger'),
            clock_timestamp()
        FROM old_table o
        WHERE (to_jsonb(o) ->> v_pk_col) ~ c_uuid_re;
    END IF;

    RETURN NULL;  -- AFTER STATEMENT trigger: return value is ignored.
END;
$fn$;

COMMENT ON FUNCTION core.fn_entity_change_log_capture() IS
    'FraiseQL #366 suppressible external-write capture. Suppresses when fraiseql.cdc_mediated=on (app-path writes); otherwise writes Debezium-enveloped core.tb_entity_change_log rows for external writes. Install per table via statement-level transition-table triggers (see fraiseql generate capture-triggers).';
