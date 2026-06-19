# Persistent identity store

FraiseQL can persist user accounts and their linked provider identities in
PostgreSQL via `PostgresAccountStore`, so account-linking state survives a process
restart. It is the durable backend for the same `AccountStore` trait the in-memory
store implements — a drop-in replacement.

## Why

The default `InMemoryAccountStore` keeps account-linking in process memory and loses
it on restart. A durable store is the spine the rest of the identity surface hangs
off (local-password credentials, password reset, social auto-linking, SCIM
provisioning).

## Schema

Two tables in the `core` schema, created idempotently by `PostgresAccountStore::init()`:

| Table | Purpose |
| --- | --- |
| `core.tb_user` | One row per stable account: `user_id` (the `"user_<uuid>"` identifier shared with `_system.sessions.user_id`), optional verified `email`, `tenant_id`. |
| `core.tb_auth_identity` | One row per linked `(provider, provider_id)`, FK to `tb_user`. `UNIQUE (provider, provider_id)` makes a provider login resolve to exactly one account. |

Account linking is identical to the in-memory semantics: a **verified, non-empty**
email links across providers; an absent/unverified email keys the identity on
`(provider, provider_id)` so distinct identities can never collapse (H26).

## Tenant isolation (RLS)

Both tables carry a `tenant_id` and Row-Level Security **deny-by-default**, mirroring
the change-log RLS (observers migration `12`):

- `ENABLE`, not `FORCE` — the store runs as the table **owner** and bypasses the
  policies (it is the trusted login path), exactly like the executor/poller for the
  change-log. A non-owner, non-`BYPASSRLS` role reads **zero** rows unless it has set
  the `fraiseql.tenant_id` GUC to a row's tenant.
- `REVOKE ALL … FROM PUBLIC` — never world-readable.

v1 operates **single-tenant** (`tenant_id` left `NULL`, since the `AccountStore`
trait carries no tenant parameter). Per-tenant scoping — threading `tenant_id` through
the trait and stamping it on write — is a forward-compatible extension; the schema and
RLS policies are already in place for it.

## Usage

```rust
use std::sync::Arc;
use fraiseql_auth::{AccountStore, PostgresAccountStore};
use sqlx::postgres::PgPoolOptions;

let pool = PgPoolOptions::new().connect(&database_url).await?;
let store = PostgresAccountStore::new(pool);
store.init().await?; // idempotent; creates core.tb_user / core.tb_auth_identity

// Hand it to the auth flows in place of InMemoryAccountStore:
let store: Arc<dyn AccountStore> = Arc::new(store);
```

The connecting role must **own** (or carry `BYPASSRLS` for) the two tables — calling
`init()` on startup creates them, so the connecting role owns them by construction.
