-- Multi-tenant example: the database half of tenant isolation.
--
-- FraiseQL resolves `[[session_variables.variables]]` from the caller's validated
-- JWT and applies each one with `set_config(name, value, true)` — transaction
-- scoped — before every query. The policies below read those settings. Nothing in
-- the GraphQL layer decides which rows a caller may see; PostgreSQL does.
--
-- Apply with:
--   psql "$DATABASE_URL" -f sql/01_schema.sql
--
-- Then connect the application as `multitenant_app` (created below), NOT as a
-- superuser: PostgreSQL skips every policy for roles with BYPASSRLS or SUPERUSER,
-- so an app running as one has no isolation at all no matter how the policies read.

BEGIN;

-- ── Tables ───────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS tb_organization (
    id   uuid PRIMARY KEY,
    data jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_tenant (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES tb_organization (id),
    data            jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_resource (
    id        uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tb_tenant (id),
    data      jsonb NOT NULL
);

-- ── Row-Level Security ───────────────────────────────────────────────────────
--
-- FORCE, not just ENABLE: without FORCE the table owner is exempt, so an
-- application connecting as the owner would see everything while the policies
-- looked correct.
--
-- NULLIF(..., '') is not decoration. `set_config(..., true)` is transaction-local,
-- so on a pooled connection that has already served a request the setting reverts
-- to the empty string rather than disappearing — and ''::uuid raises 22P02, turning
-- an unauthenticated read into a 500 instead of an empty result set.

ALTER TABLE tb_organization ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_organization FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS organization_isolation ON tb_organization;
CREATE POLICY organization_isolation ON tb_organization
    USING (id = NULLIF(current_setting('app.organization_id', true), '')::uuid);

ALTER TABLE tb_tenant ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_tenant FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON tb_tenant;
CREATE POLICY tenant_isolation ON tb_tenant
    USING (organization_id = NULLIF(current_setting('app.organization_id', true), '')::uuid);

ALTER TABLE tb_resource ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_resource FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS resource_isolation ON tb_resource;
CREATE POLICY resource_isolation ON tb_resource
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

-- ── Views ────────────────────────────────────────────────────────────────────
--
-- `security_invoker = true` (PostgreSQL 15+) is load-bearing. A default view runs
-- with its *owner's* privileges, which bypasses the caller's policies entirely — a
-- view over a perfectly protected table would return every tenant's rows. FraiseQL
-- refuses to boot on a non-invoker view when `[security.rls] enabled = true`.

-- The `data` column is the entity: FraiseQL projects GraphQL fields out of its
-- JSON keys, not out of table columns. The identity and foreign-key columns are
-- merged in under their snake_case names (the default `naming_convention =
-- "camelCase"` maps GraphQL `tenantId` to the JSON key `tenant_id`), so they are
-- addressable in a query without duplicating
-- them into every row's payload at write time.

CREATE OR REPLACE VIEW v_organization WITH (security_invoker = true) AS
    SELECT id, jsonb_build_object('id', id::text) || data AS data
    FROM tb_organization;

CREATE OR REPLACE VIEW v_tenant WITH (security_invoker = true) AS
    SELECT id, organization_id,
           jsonb_build_object('id', id::text, 'organization_id', organization_id::text) || data
             AS data
    FROM tb_tenant;

CREATE OR REPLACE VIEW v_resource WITH (security_invoker = true) AS
    SELECT id, tenant_id,
           jsonb_build_object('id', id::text, 'tenant_id', tenant_id::text) || data AS data
    FROM tb_resource;

-- ── Application role ─────────────────────────────────────────────────────────

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'multitenant_app') THEN
        CREATE ROLE multitenant_app LOGIN PASSWORD 'multitenant_app';
    END IF;
END $$;

GRANT USAGE ON SCHEMA public TO multitenant_app;
GRANT SELECT ON tb_organization, tb_tenant, tb_resource TO multitenant_app;
GRANT SELECT ON v_organization, v_tenant, v_resource TO multitenant_app;

COMMIT;
