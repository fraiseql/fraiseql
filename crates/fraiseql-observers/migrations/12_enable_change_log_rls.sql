-- FraiseQL Change-Log Row-Level Security — core.tb_entity_change_log
-- ============================================================================
-- Closes audit #437 finding F6 (#443): `core.tb_entity_change_log` holds the
-- full before/after entity payload for EVERY tenant, and until this migration any
-- database role with `SELECT` on the table (or its two views) could read all of
-- them. The contract comments called `tenant_id` an "RLS partition stamp" and the
-- per-tenant index "RLS-clean", but RLS was never actually enabled.
--
-- What this migration enforces (and what it does NOT)
-- ---------------------------------------------------
-- The change-log is a deliberately CROSS-TENANT operational surface: the poller
-- and the three NATS bridges fan out every tenant's events (per-tenant routing
-- happens at subscription-match time, keyed on the event's `tenant_id`), and the
-- GraphQL `entity_change_logs` query + the admin HTTP handlers are trusted,
-- role-gated, all-tenant readers. FraiseQL does NOT set a `fraiseql.tenant_id`
-- session GUC on its read paths today (row-mode tenancy uses WHERE-clause
-- injection; schema-mode uses `SET search_path`). So the practical effect of the
-- SELECT policy below is **deny-by-default**: a role that is neither the table
-- owner nor `BYPASSRLS`, and that has not set `fraiseql.tenant_id`, reads ZERO
-- change-log rows. The per-tenant `tenant_id = current_setting(...)` shape is
-- forward-looking — it lets a future per-tenant reader (one that sets the GUC)
-- see exactly its own tenant — but it is not exercised by any current FraiseQL
-- code path. The security win today is the fail-closed deny-by-default.
--
-- Operator action (BREAKING)
-- --------------------------
-- The trusted internal consumers — the poller, the 3 NATS bridges, the server
-- changelog HTTP handlers, and the mutation executor's outbox INSERT — all run on
-- the server's database role. That role MUST be the table owner or carry
-- `BYPASSRLS`, otherwise the CDC pipeline and the admin change-log query silently
-- return an empty result. (CI's superuser role bypasses RLS automatically, which
-- is exactly why the isolation test in `tests/rls_isolation.rs` runs the
-- assertions under a dedicated NOBYPASSRLS role — a superuser would mask the
-- policy entirely.)
--
-- `ENABLE`, not `FORCE`
-- ---------------------
-- Under `ENABLE ROW LEVEL SECURITY` the table owner and any `BYPASSRLS` role skip
-- all policies. That is required: the `SECURITY DEFINER` capture function
-- (migration 11) runs as the owner so external-write capture keeps working, and
-- the trusted consumers run as `BYPASSRLS`. `FORCE` would break both.
--
-- The two read views (`core.v_entity_change_log`,
-- `core.v_entity_change_log_debezium`) do NOT inherit the base-table RLS by
-- default: a plain PostgreSQL view runs its underlying query as the view OWNER
-- (here the migration deployer / superuser), so it would read every tenant's rows
-- regardless of the querying role's policy. `security_invoker = true`
-- (PostgreSQL 15+) makes a view run as the QUERYING role, so it honours both the
-- base-table policy and the role's `fraiseql.tenant_id` GUC — that flag is set
-- below. On PostgreSQL < 15 the option does not exist; the views stay owner-run
-- and MUST be protected by restricting `SELECT` on them to trusted roles (the
-- migration RAISEs a WARNING to that effect).
--
-- PostgreSQL only. Idempotent / re-run safe (ENABLE is a no-op when already on;
-- DROP POLICY IF EXISTS + CREATE POLICY replaces cleanly). MySQL / SQL Server
-- change-log isolation is a tracked follow-up (their bridges read the unscoped
-- table directly).

CREATE SCHEMA IF NOT EXISTS core;

ALTER TABLE core.tb_entity_change_log ENABLE ROW LEVEL SECURITY;  -- NOT FORCE

-- Read isolation (deny-by-default; forward-compat per-tenant). A non-owner,
-- non-BYPASSRLS role sees a row only when it has set `fraiseql.tenant_id` to that
-- row's tenant. `NULLIF(..., '')` maps both an unset GUC and an empty string to
-- NULL, so `tenant_id = NULL` is NULL → the row is hidden (fail-closed) rather
-- than raising on an `''::uuid` cast.
DROP POLICY IF EXISTS p_change_log_tenant_read ON core.tb_entity_change_log;
CREATE POLICY p_change_log_tenant_read ON core.tb_entity_change_log
    FOR SELECT
    USING (
        tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid
    );

-- Permissive write: the executor outbox INSERT (server role) and the
-- `SECURITY DEFINER` capture function are trusted to stamp the correct
-- `tenant_id`. A permissive `WITH CHECK (true)` never rejects an anonymous
-- external-write capture (which legitimately has a NULL tenant), while the SELECT
-- policy above still governs who can read those rows back.
DROP POLICY IF EXISTS p_change_log_insert ON core.tb_entity_change_log;
CREATE POLICY p_change_log_insert ON core.tb_entity_change_log
    FOR INSERT
    WITH CHECK (true);

-- Make the two read views run as the querying role (PostgreSQL 15+), so they
-- enforce the base-table RLS above instead of bypassing it as the owner. Guarded
-- on the server version: on < 15 the option does not exist, so warn that the views
-- must be access-restricted instead. Idempotent (SET is re-run safe).
DO $$
BEGIN
    IF current_setting('server_version_num')::int >= 150000 THEN
        EXECUTE 'ALTER VIEW core.v_entity_change_log SET (security_invoker = true)';
        EXECUTE 'ALTER VIEW core.v_entity_change_log_debezium SET (security_invoker = true)';
    ELSE
        RAISE WARNING
            'PostgreSQL % (< 15): change-log views cannot use security_invoker and will '
            'bypass the base-table RLS. Restrict SELECT on core.v_entity_change_log and '
            'core.v_entity_change_log_debezium to trusted / BYPASSRLS roles.',
            current_setting('server_version');
    END IF;
END $$;
