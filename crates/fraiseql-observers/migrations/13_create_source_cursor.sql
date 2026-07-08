-- FraiseQL Source Cursor Store — _fraiseql_source_cursor
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
-- PostgreSQL only. Idempotent / re-run safe (CREATE TABLE IF NOT EXISTS; ENABLE is
-- a no-op when already on; DROP POLICY IF EXISTS + CREATE POLICY replaces cleanly;
-- REVOKE is idempotent).

CREATE TABLE IF NOT EXISTS _fraiseql_source_cursor (
    source_name  TEXT        PRIMARY KEY,
    cursor_value JSONB,
    version      BIGINT      NOT NULL DEFAULT 0,
    tenant_id    UUID,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE _fraiseql_source_cursor ENABLE ROW LEVEL SECURITY;  -- NOT FORCE

-- Read isolation (deny-by-default; forward-compat per-tenant).
DROP POLICY IF EXISTS p_source_cursor_read ON _fraiseql_source_cursor;
CREATE POLICY p_source_cursor_read ON _fraiseql_source_cursor
    FOR SELECT
    USING (
        tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid
    );

-- Permissive writes: the trusted source scheduler (server role) stamps the correct
-- tenant. The SELECT policy above still governs reads; REVOKE FROM PUBLIC below
-- keeps untrusted roles out entirely.
DROP POLICY IF EXISTS p_source_cursor_insert ON _fraiseql_source_cursor;
CREATE POLICY p_source_cursor_insert ON _fraiseql_source_cursor
    FOR INSERT
    WITH CHECK (true);

DROP POLICY IF EXISTS p_source_cursor_update ON _fraiseql_source_cursor;
CREATE POLICY p_source_cursor_update ON _fraiseql_source_cursor
    FOR UPDATE
    USING (true)
    WITH CHECK (true);

-- Least-privilege baseline: never world-readable.
REVOKE ALL ON _fraiseql_source_cursor FROM PUBLIC;
