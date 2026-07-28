-- SaaS example: the database half of account isolation.
--
-- FraiseQL resolves `[[session_variables.variables]]` from the caller's validated
-- JWT and applies each with `set_config(name, value, true)` — transaction scoped —
-- before every query and mutation. The policies below read those settings. Nothing
-- in the GraphQL layer decides which rows a caller may see; PostgreSQL does.
--
-- Apply with:
--   psql "$DATABASE_URL" -f sql/01_schema.sql
--
-- Then connect the application as `saas_app` (created below), NOT as a superuser:
-- PostgreSQL skips every policy for roles with BYPASSRLS or SUPERUSER, so an app
-- running as one has no isolation at all no matter how the policies read.

BEGIN;

-- ── Tables ───────────────────────────────────────────────────────────────────
--
-- Every table carries `account_id`, including the two whose GraphQL type reaches
-- the account indirectly (`WebhookLog` via Integration, `TeamMember` via Team).
-- Denormalising the isolation key is deliberate: a policy that has to join to find
-- the account is a policy that can be defeated by a missing index or a recursive
-- lookup the planner declines to inline.

CREATE TABLE IF NOT EXISTS tb_account (
    id       uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    data     jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_account_user (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_subscription (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_invoice (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_integration (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_webhook_log (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_team (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS tb_team_member (
    id         uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES tb_account (id),
    data       jsonb NOT NULL
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

CREATE OR REPLACE FUNCTION current_account_id() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('app.account_id', true), '')::uuid
$$;

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'tb_account_user', 'tb_subscription', 'tb_invoice',
        'tb_integration', 'tb_webhook_log', 'tb_team', 'tb_team_member'
    ] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('DROP POLICY IF EXISTS account_isolation ON %I', t);
        EXECUTE format(
            'CREATE POLICY account_isolation ON %I USING (account_id = current_account_id())', t
        );
    END LOOP;
END $$;

ALTER TABLE tb_account ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_account FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS account_isolation ON tb_account;
CREATE POLICY account_isolation ON tb_account
    USING (id = current_account_id());

-- The write side: this is what the removed `[[security.rules]]` blocks — "only
-- account owners can modify Account", "only billing admins can manage
-- Subscription" — claimed to enforce and did not. Expressed as `WITH CHECK`
-- policies they are enforced by PostgreSQL on every write, including writes that
-- do not go through FraiseQL at all.
DROP POLICY IF EXISTS account_owner_writes ON tb_account;
CREATE POLICY account_owner_writes ON tb_account
    FOR UPDATE
    USING (id = current_account_id())
    WITH CHECK (owner_id = NULLIF(current_setting('app.user_id', true), '')::uuid);

DROP POLICY IF EXISTS billing_admin_writes ON tb_subscription;
CREATE POLICY billing_admin_writes ON tb_subscription
    FOR UPDATE
    USING (account_id = current_account_id())
    WITH CHECK (
        account_id = current_account_id()
        AND current_setting('app.account_role', true) IN ('owner', 'billing_admin')
    );

-- ── Views ────────────────────────────────────────────────────────────────────
--
-- `security_invoker = true` (PostgreSQL 15+) is load-bearing. A default view runs
-- with its *owner's* privileges, which bypasses the caller's policies entirely — a
-- view over a perfectly protected table would return every account's rows. FraiseQL
-- refuses to boot on a non-invoker view when `[security.rls] enabled = true`.

-- The `data` column is the entity: FraiseQL projects GraphQL fields out of its
-- JSON keys, not out of table columns. `id` and `account_id` are merged in — the
-- default `naming_convention = "camelCase"` maps the GraphQL field `accountId` to
-- the JSON key `account_id` — so they are addressable in a query without
-- duplicating them into every payload at write time.

CREATE OR REPLACE VIEW v_account WITH (security_invoker = true) AS
    SELECT id, owner_id,
           jsonb_build_object('id', id::text, 'owner_id', owner_id::text) || data AS data
    FROM tb_account;

CREATE OR REPLACE VIEW v_account_user WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text, 'account_id', account_id::text) || data AS data
    FROM tb_account_user;

CREATE OR REPLACE VIEW v_subscription WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text, 'account_id', account_id::text) || data AS data
    FROM tb_subscription;

CREATE OR REPLACE VIEW v_invoice WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text, 'account_id', account_id::text) || data AS data
    FROM tb_invoice;

CREATE OR REPLACE VIEW v_integration WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text, 'account_id', account_id::text) || data AS data
    FROM tb_integration;

CREATE OR REPLACE VIEW v_webhook_log WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text) || data AS data
    FROM tb_webhook_log;

CREATE OR REPLACE VIEW v_team WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text, 'account_id', account_id::text) || data AS data
    FROM tb_team;

CREATE OR REPLACE VIEW v_team_member WITH (security_invoker = true) AS
    SELECT id, account_id,
           jsonb_build_object('id', id::text) || data AS data
    FROM tb_team_member;

-- ── Application role ─────────────────────────────────────────────────────────

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'saas_app') THEN
        CREATE ROLE saas_app LOGIN PASSWORD 'saas_app';
    END IF;
END $$;

GRANT USAGE ON SCHEMA public TO saas_app;
GRANT SELECT ON tb_account, tb_account_user, tb_subscription, tb_invoice,
                tb_integration, tb_webhook_log, tb_team, tb_team_member TO saas_app;
GRANT SELECT ON v_account, v_account_user, v_subscription, v_invoice,
                v_integration, v_webhook_log, v_team, v_team_member TO saas_app;
GRANT UPDATE ON tb_account, tb_subscription TO saas_app;
GRANT EXECUTE ON FUNCTION current_account_id() TO saas_app;

COMMIT;
