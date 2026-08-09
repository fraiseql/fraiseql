# SaaS Platform Example

A FraiseQL v2 schema for a Software-as-a-Service platform, with account isolation
that is **enforced and tested** rather than described. The isolation lives in
PostgreSQL Row-Level Security; FraiseQL's job is to put the caller's identity where
the policies can read it.

## Schema Structure

```
schema/
├── accounts/       # Account, AccountUser
├── billing/        # Subscription, Invoice
├── teams/          # Team, TeamMember
└── integrations/   # Integration, WebhookLog
sql/
└── 01_schema.sql   # tables, RLS policies, security_invoker views, app role
```

## Domains

| Domain | Types | Queries |
|---|---|---|
| Accounts | `Account`, `AccountUser` | `getAccount`, `getAccountBySlug`, `listAccountUsers` |
| Billing | `Subscription`, `Invoice` | `getSubscription`, `listInvoices` |
| Teams | `Team`, `TeamMember` | `listTeams`, `getTeam`, `listTeamMembers` |
| Integrations | `Integration`, `WebhookLog` | `listIntegrations`, `getIntegration`, `listWebhookLogs` |

Each domain is a directory under `schema/`; `[domain_discovery]` finds them all, so
adding `schema/support/types.json` is the whole of adding a Support domain.

## How account isolation actually works

**1. FraiseQL maps JWT claims to PostgreSQL session variables.**

```toml
[[session_variables.variables]]
name = "app.account_id"
source = "jwt"
claim = "account_id"
```

Before every query the runtime calls `set_config('app.account_id', <claim>, true)` —
transaction-scoped, so it cannot leak to the next request on a pooled connection.
The value comes from the *validated* JWT, so a client cannot supply it in a header
or a GraphQL variable.

**2. The database decides which rows exist.**

```sql
CREATE POLICY account_isolation ON tb_invoice
    USING (account_id = current_account_id());   -- NULLIF(current_setting(...), '')::uuid
```

A request with no account claim matches no rows. Fail-closed by construction — there
is no code path that "forgets" the filter, because the filter is not in the code.

**3. The views must defer to the caller.**
Every view is `WITH (security_invoker = true)`. A default view runs with its
*owner's* privileges and bypasses the caller's policies entirely, so a view over a
perfectly protected table hands back every account's rows. This is the easiest way
to build a multi-tenant deployment that looks isolated and is not.

## The write rules, for real this time

Earlier revisions declared `[[security.rules]]` blocks meaning "only account owners
can modify Account" and "only billing admins can manage Subscription". FraiseQL pins
its operation and field authorizers to `None`, so those blocks compiled and enforced
nothing (#612 / #626). They were removed rather than ship a false claim.

`sql/01_schema.sql` expresses the same two rules as **restrictive** policies, which
PostgreSQL enforces on every write — including writes that never go through
FraiseQL:

```sql
CREATE POLICY billing_admin_writes ON tb_subscription
    AS RESTRICTIVE
    FOR UPDATE
    USING (current_account_role() IN ('owner', 'billing_admin'))
    WITH CHECK (
        account_id = current_account_id()
        AND current_account_role() IN ('owner', 'billing_admin')
    );
```

The `app.user_id` and `app.account_role` session variables those policies read are
declared alongside `app.account_id` in `fraiseql.toml`.

**`AS RESTRICTIVE` is the whole rule, not a style choice.** PostgreSQL combines
*permissive* policies with **OR**, and a policy with `USING` but no `WITH CHECK`
reuses its `USING` expression as the `WITH CHECK` for UPDATE. `account_isolation`
is permissive, has no `FOR` clause (so it covers ALL commands) and no `WITH CHECK`
— so written as permissive policies, both rules above were OR'd with
`account_id = current_account_id()`, which is true for every member of the account.
They enforced nothing, which is the same failure mode as the `[[security.rules]]`
blocks they replaced, one layer down. Restrictive policies AND with the permissive
set, which is what makes them conditions rather than alternatives.

Two traps worth naming, because both look like fixes:

- Making `account_isolation` itself restrictive leaves the table with **no**
  permissive policy, so every `SELECT` returns zero rows.
- Putting the ownership test only in `WITH CHECK` is not enough. A member could then
  update the account row and satisfy the check by setting `owner_id` **to
  themselves** — self-promotion, after which the rule is satisfied forever. Which
  existing rows a caller may write is a `USING` question.

These rules are scoped `FOR UPDATE` because UPDATE is the only write granted to
`saas_app`. Granting INSERT or DELETE means writing the matching restrictive policy;
otherwise cmd=ALL `account_isolation` governs that command by itself and nullifies
any new rule the same way.

## Two things that will silently defeat this

**Connecting as a superuser or a `BYPASSRLS` role.** PostgreSQL skips every policy
for such roles. The application must connect as an ordinary role — `sql/01_schema.sql`
creates `saas_app` for exactly this.

**Being the table owner without `FORCE ROW LEVEL SECURITY`.** `ENABLE` alone exempts
the owner. Every table here uses `FORCE`.

## What FraiseQL checks for you

```toml
[security]
multi_tenant = true

[security.rls]
enabled = true
```

`multi_tenant = true` activates the subscription tenant fail-closed gate and the
cache+RLS boot gate. `[security.rls] enabled = true` is a claim, so the server
verifies it at boot against the live catalog: every relation a query reads must be
an RLS-protected table or a `security_invoker` view, or startup fails naming the
relation.

## Mutations

This example declares **queries only**. It previously declared eight mutations with
no input type and no backing SQL function — the compiler accepted them and nothing
could ever execute one. They are removed rather than left as a shape that looks
supported. For the mutation story see `examples/mutation-patterns`.

## Running it

```bash
createdb saas
psql "postgresql://localhost/saas" -f sql/01_schema.sql
fraiseql compile fraiseql.toml
DATABASE_URL="postgresql://saas_app:saas_app@localhost/saas" \
  fraiseql run --config fraiseql.toml
```

## Production considerations

1. **Indexes.** Add an index on `account_id` for every table — every policy
   references it, so it is on the hot path of every query.
2. **Connection role.** Never connect as the owner or a `BYPASSRLS` role. This is
   the single most common way a correctly-policied database leaks.
3. **Data deletion.** Per-account deletion is a `DELETE ... WHERE account_id = $1`
   across the eight tables; the foreign keys give you the ordering.

## The proof

`crates/fraiseql-server/tests/example_multitenant_rls_e2e_pg.rs` applies both
examples' SQL to a real PostgreSQL, compiles both schemas through the real compile
path, seeds two accounts, and asserts each sees only its own rows and an
unauthenticated caller sees none. It runs in CI's `integration` leg. If the
isolation story here stops being true, that test goes red.

The **write** rules have their own case in the same file. It reads nothing through
GraphQL — this example declares no mutations, and the claim above is specifically
about writes that never go through FraiseQL — so it issues direct SQL as `saas_app`,
the role this README tells you to connect as, and asserts a plain member can change
neither the subscription nor the account nor their own membership into ownership,
while a billing admin and the owner still can. That case did not exist until #1070,
which is why both rules could be decorative without anything going red.

See `../../docs/DOMAIN_ORGANIZATION.md` for more on domain organisation.
