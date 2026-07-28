# Multi-Tenant Example

A FraiseQL v2 example whose tenant isolation is **enforced and tested**, not
documented. The isolation lives in PostgreSQL Row-Level Security; FraiseQL's job is
to put the caller's identity where the policies can read it.

## Structure

```
schema/
├── core/        # Organization
├── tenants/     # Tenant (organizationId)
└── resources/   # Resource (tenantId)
sql/
└── 01_schema.sql   # tables, RLS policies, security_invoker views, app role
```

## How isolation actually works

Three pieces, none of which works alone.

**1. FraiseQL maps JWT claims to PostgreSQL session variables.**
`fraiseql.toml` declares:

```toml
[[session_variables.variables]]
name = "app.tenant_id"
source = "jwt"
claim = "tenant_id"
```

Before every query and mutation the runtime calls
`set_config('app.tenant_id', <claim value>, true)` — transaction-scoped, so it
cannot leak to the next request on a pooled connection. The value comes from the
*validated* JWT, so a client cannot supply it in a header or a GraphQL variable.

**2. The database decides which rows exist.**

```sql
CREATE POLICY resource_isolation ON tb_resource
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
```

A request with no tenant claim sets nothing, `current_setting` returns the empty
string, `NULLIF` makes it NULL, and the comparison matches no rows. Fail-closed by
construction — there is no code path that "forgets" the filter, because the filter
is not in the code.

**3. The views must defer to the caller.**
`v_resource` is declared `WITH (security_invoker = true)`. A default view executes
with its *owner's* privileges and bypasses the caller's policies entirely, so a view
over a perfectly protected table would hand back every tenant's rows. This is the
single easiest way to build a multi-tenant deployment that looks isolated and is not.

## Two things that will silently defeat this

**Connecting as a superuser or a `BYPASSRLS` role.** PostgreSQL skips every policy
for such roles. The application must connect as an ordinary role —
`sql/01_schema.sql` creates `multitenant_app` for exactly this. This is why the
example ships an application role at all.

**Being the table owner without `FORCE ROW LEVEL SECURITY`.** `ENABLE` alone exempts
the owner. Every table here uses `FORCE`.

## What FraiseQL checks for you

`fraiseql.toml` declares both halves:

```toml
[security]
multi_tenant = true

[security.rls]
enabled = true
```

`multi_tenant = true` turns on the subscription tenant fail-closed gate and the
cache+RLS boot gate: with caching enabled and no RLS declared, the server refuses to
start rather than risk serving one tenant a response computed for another.

`[security.rls] enabled = true` is a claim, so the server checks it at boot against
the live catalog: every relation a query reads must be an RLS-protected table or a
`security_invoker` view. A missing policy, a view that forgot `security_invoker`, or
a relation that does not exist is a startup failure naming the relation — not a leak
discovered in production.

## Running it

```bash
createdb multitenant
psql "postgresql://localhost/multitenant" -f sql/01_schema.sql
fraiseql compile fraiseql.toml
DATABASE_URL="postgresql://multitenant_app:multitenant_app@localhost/multitenant" \
  fraiseql run --config fraiseql.toml
```

## The proof

`crates/fraiseql-server/tests/example_multitenant_rls_e2e_pg.rs` compiles this
example, applies `sql/01_schema.sql` to a real PostgreSQL, seeds two tenants, and
asserts that each sees only its own rows and an unauthenticated caller sees none.
It runs in CI's `integration` leg. If the isolation story here stops being true, that
test goes red.

## Historical note (#612, #628)

Earlier revisions of this example claimed `[[security.rules]]` in `fraiseql.toml`
scoped each query to the caller's tenant. That was false — FraiseQL pins the
operation/field authorizers to `None`, so the blocks compiled and enforced nothing.
They were removed under #612, leaving the example with an honest description of a
mechanism it did not demonstrate. #628 is the other half: the mechanism, wired,
with a test.
