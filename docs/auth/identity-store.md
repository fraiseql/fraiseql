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
| `core.tb_auth_identity` | One row per linked `(provider, provider_id)`, FK to `tb_user`. `(provider, provider_id)` is unique per account space, so a provider login resolves to exactly one account in it. |

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

## Account spaces (#1088)

`tenant_id` partitions accounts. `NULL` is the **platform**; a UUID is that **tenant**.
Email, SCIM `userName` and `(provider, provider_id)` are each unique *within* a space, so
the same address in two tenants is two accounts, and an email merge never crosses a space.
`link_or_create_user(tenant, …)` confines every lookup and insert to `tenant`.

Who decides the space is the security property. A tenant account is created only by an
**authority of that tenant**:

| Path | Account space |
| --- | --- |
| SAML sign-in through a tenant-bound IdP | that IdP's `tenant_id` |
| SCIM provisioning with a tenant-scoped token | that token's tenant |
| Password, email OTP, phone OTP, social OAuth | platform |

Nothing the client sends picks the space. A header or a body field naming a tenant would let
anyone sign up into, or link into, any tenant.

Each key is one unique index over `(key, tenant_id) NULLS NOT DISTINCT`, so the platform
(`tenant_id IS NULL`) is a space like any tenant. `init` migrates an older database in
place: it drops the global keys an earlier release created and installs the per-space
indexes. No row moves, and existing accounts stay platform accounts.

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
