-- FraiseQL Source Cursor Store — core._fraiseql_source_cursor
-- ============================================================================
-- The durable watermark for scheduled ingress `Source`s (#573, the dual of
-- `Observer`). A source polls an external system on a schedule and drives the
-- results into the database via mutations; this table holds the opaque cursor it
-- advances between runs so a re-run resumes from the last committed watermark
-- (at-least-once, cursor-gated). One row per source, keyed by its stable name.
--
-- Columns
-- -------
--   source_name  — stable source identifier and primary key (one cursor per source).
--   cursor_value — OPAQUE JSONB the source owns end to end (e.g. an IMAP
--                  {uid_validity,last_uid}, an API page token, a high-water
--                  timestamp). The framework never interprets it; it is written
--                  only via parameterized binds, never assembled into SQL text.
--   version      — a monotonic generation counter the store bumps on every
--                  advance. It is the compare-and-swap guard: an advance that read
--                  version N only applies while the row is still at N, so a stale
--                  writer (one that lost the single-firing lease across a failover)
--                  can never regress the watermark.
--   tenant_id    — RLS partition stamp (Trinity: the JWT/tenant stamp, NOT a
--                  business FK). NULL for a global/system source. See RLS below.
--   updated_at   — last advance time (observability / lag).
--
-- Row-Level Security — deny-by-default, mirroring migration 12
-- -----------------------------------------------------------
-- The cursor can encode externally-derived positions, so the table is fail-closed
-- exactly like `core.tb_entity_change_log`: a role that is neither the table owner
-- nor `BYPASSRLS`, and that has not set the `fraiseql.tenant_id` session GUC, reads
-- ZERO rows. `NULLIF(current_setting(..., true), '')::uuid` maps both an unset GUC
-- and an empty string to NULL, so `tenant_id = NULL` is NULL → the row is hidden
-- (never an `''::uuid` cast error). A global source stamps `tenant_id = NULL`, so
-- its cursor is likewise invisible to any non-BYPASSRLS role — deny-by-default.
--
-- Operator action (matches migration 12): the source scheduler runs on the
-- server's database role, which MUST be the table owner or carry `BYPASSRLS` (the
-- same requirement the change-log poller already imposes). `ENABLE`, not `FORCE`,
-- so the owner is exempt. The permissive INSERT/UPDATE policies let a granted,
-- non-BYPASSRLS role manage cursors too; the SELECT policy above still governs who
-- can read them back. FraiseQL does not set `fraiseql.tenant_id` on any read path
-- today, so the practical effect is deny-by-default; the per-tenant shape is
-- forward-looking for a future per-tenant reader.
--
-- Least-privilege baseline: `REVOKE ALL … FROM PUBLIC` so the cursor store is never
-- world-readable and RLS is genuine defence-in-depth on top of grants rather than
-- the sole control. PostgreSQL grants no PUBLIC privileges to a fresh table, so
-- this is a defensive no-op on a clean install and a real tightening where a prior
-- broad `GRANT … TO PUBLIC` reached it. Idempotent.
--
-- PostgreSQL only. Idempotent / re-run safe, and a boot over a current schema takes no
-- table lock: CREATE TABLE IF NOT EXISTS; RLS and each policy behind a catalog guard (a
-- policy whose definition differs is replaced); REVOKE is idempotent.

-- The table lives in `core` whatever the connection's search_path, as the observer
-- tables do: created unqualified, it landed in whichever schema led the path, and a
-- deployment whose path changed read an empty table and restarted every source.
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core._fraiseql_source_cursor (
    source_name  TEXT        PRIMARY KEY,
    cursor_value JSONB,
    version      BIGINT      NOT NULL DEFAULT 0,
    tenant_id    UUID,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- `ENABLE ROW LEVEL SECURITY`, `DROP POLICY` and `CREATE POLICY` each take an ACCESS
-- EXCLUSIVE lock even when they change nothing, and every replica runs this at boot. So each
-- is guarded by the catalog: RLS by `relrowsecurity`, each policy by its whole definition in
-- `pg_policies`, and a policy whose definition differs is replaced. `tenant_read` is the read
-- policy's expression as PostgreSQL 18 (the supported floor) stores it; should it differ from
-- what the CREATE below stores, every boot replaces the policy under its lock and the
-- boot-lock test fails.
DO $source_cursor_rls$
DECLARE
    tenant_read CONSTANT text :=
        $q$(tenant_id = (NULLIF(current_setting('fraiseql.tenant_id'::text, true), ''::text))::uuid)$q$;
BEGIN
    IF NOT (SELECT relrowsecurity FROM pg_class WHERE oid = 'core._fraiseql_source_cursor'::regclass) THEN
        ALTER TABLE core._fraiseql_source_cursor ENABLE ROW LEVEL SECURITY;  -- NOT FORCE
    END IF;

    -- Read isolation (deny-by-default; forward-compat per-tenant).
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'core' AND tablename = '_fraiseql_source_cursor'
          AND policyname = 'p_source_cursor_read'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'SELECT'
          AND qual = tenant_read AND with_check IS NULL
    ) THEN
        DROP POLICY IF EXISTS p_source_cursor_read ON core._fraiseql_source_cursor;
        CREATE POLICY p_source_cursor_read ON core._fraiseql_source_cursor
            FOR SELECT
            USING (
                tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid
            );
    END IF;

    -- Permissive writes: the trusted source scheduler (server role) stamps the correct
    -- tenant. The SELECT policy above still governs reads; REVOKE FROM PUBLIC below
    -- keeps untrusted roles out entirely.
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'core' AND tablename = '_fraiseql_source_cursor'
          AND policyname = 'p_source_cursor_insert'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'INSERT'
          AND qual IS NULL AND with_check = 'true'
    ) THEN
        DROP POLICY IF EXISTS p_source_cursor_insert ON core._fraiseql_source_cursor;
        CREATE POLICY p_source_cursor_insert ON core._fraiseql_source_cursor
            FOR INSERT
            WITH CHECK (true);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'core' AND tablename = '_fraiseql_source_cursor'
          AND policyname = 'p_source_cursor_update'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'UPDATE'
          AND qual = 'true' AND with_check = 'true'
    ) THEN
        DROP POLICY IF EXISTS p_source_cursor_update ON core._fraiseql_source_cursor;
        CREATE POLICY p_source_cursor_update ON core._fraiseql_source_cursor
            FOR UPDATE
            USING (true)
            WITH CHECK (true);
    END IF;
END
$source_cursor_rls$;

-- Least-privilege baseline: never world-readable.
REVOKE ALL ON core._fraiseql_source_cursor FROM PUBLIC;
