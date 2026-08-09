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

-- The other two claims the write policies below read. Same NULLIF discipline, and
-- the same fail-closed consequence: an unset setting yields NULL, every comparison
-- against NULL is NULL rather than true, and the policy denies.

CREATE OR REPLACE FUNCTION current_app_user_id() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('app.user_id', true), '')::uuid
$$;

CREATE OR REPLACE FUNCTION current_account_role() RETURNS text
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('app.account_role', true), '')
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
-- Subscription" — claimed to enforce and did not. Expressed as policies they are
-- enforced by PostgreSQL on every write, including writes that do not go through
-- FraiseQL at all.
--
-- `AS RESTRICTIVE` is load-bearing, and its absence is what made these two rules
-- decorative for a second time (#1070). PostgreSQL combines *permissive* policies
-- with OR, and a policy with `USING` but no `WITH CHECK` reuses its `USING`
-- expression as the `WITH CHECK` for UPDATE. `account_isolation` above is
-- permissive, carries no `FOR` clause (so it applies to ALL commands) and no
-- `WITH CHECK` — so as permissive policies these two rules were OR'd with
-- `account_id = current_account_id()`, which is true for every member of the
-- account, and neither ever fired.
--
-- Cross-account isolation — this example's headline property, and the one the e2e
-- asserts — was never affected: the surviving disjunct still pins the row to the
-- caller's own account. What was nullified is the *intra*-account role rule, so any
-- member of account A could rewrite A's subscription; and because the ALL policy's
-- implicit check on `tb_account` is only `id = current_account_id()` while the
-- UPDATE grant below names no column list, a member could rewrite `owner_id` itself
-- and self-promote, after which even the owner rule would be satisfied for good.
--
-- Restrictive policies AND with the permissive set instead of OR-ing into it, which
-- is what makes these conjunctions rather than alternatives. Note the tempting wrong
-- fix: making `account_isolation` restrictive would leave both tables with no
-- permissive policy at all, and every SELECT would return zero rows.
--
-- They are scoped `FOR UPDATE` because UPDATE is the only write granted to
-- `saas_app`. Granting INSERT or DELETE means adding the matching restrictive
-- policy — otherwise `account_isolation`, being cmd=ALL, governs that command alone
-- and nullifies any new rule exactly as it did these.
DROP POLICY IF EXISTS account_owner_writes ON tb_account;
CREATE POLICY account_owner_writes ON tb_account
    AS RESTRICTIVE
    FOR UPDATE
    -- Which existing rows may be updated: only the account's own owner may. This has
    -- to be the USING side. With the ownership test only in WITH CHECK, a member
    -- could update the row *and* satisfy the check by setting `owner_id` to
    -- themselves — the self-promotion above.
    USING (owner_id = current_app_user_id())
    -- And what the row may become: it must stay in the caller's account. Handing
    -- ownership to someone else is a legitimate thing for the current owner to do,
    -- so `owner_id` is deliberately not re-pinned here.
    WITH CHECK (id = current_account_id());

DROP POLICY IF EXISTS billing_admin_writes ON tb_subscription;
CREATE POLICY billing_admin_writes ON tb_subscription
    AS RESTRICTIVE
    FOR UPDATE
    USING (current_account_role() IN ('owner', 'billing_admin'))
    WITH CHECK (
        account_id = current_account_id()
        AND current_account_role() IN ('owner', 'billing_admin')
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
GRANT EXECUTE ON FUNCTION current_app_user_id() TO saas_app;
GRANT EXECUTE ON FUNCTION current_account_role() TO saas_app;

COMMIT;
