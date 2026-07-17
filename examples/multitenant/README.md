# Multi-Tenant Example

A minimal FraiseQL v2 example showing multi-tenant architecture.

## Structure

```
schema/
├── core/        # Shared types (Organization)
├── tenants/     # Tenant management
└── resources/   # Tenant-specific resources
```

## Domains

**Core**: Shared Organization type
**Tenants**: Workspace/tenant management
**Resources**: Application resources per tenant

## Key Pattern

All tenant-specific types include `tenantId`:

```
Organization
└── Tenant (organizationId)
    └── Resource (tenantId)
```

## Tenant isolation

> **Correction (#612).** Earlier revisions of this example claimed
> `[[security.rules]]` in `fraiseql.toml` scoped each query to the caller's tenant.
> That was false: FraiseQL does **not** enforce `[security.rules]` at runtime (the
> operation/field authorizers are pinned to `None` — see #612 / #626). Those blocks
> compiled but scoped nothing, so they have been removed. Do not rely on
> `[security.rules]` for isolation.

Per-tenant isolation must be enforced **at the database layer**. The schema itself
is unaware of who is calling — every type carries a `tenantId` (`organizationId`
for `Tenant`), but nothing scopes a query to one tenant on its own.

`default_policy = "authenticated"` closes the anonymous read path (no unauthenticated
access) — but it does **not** scope rows to a tenant. To scope rows:

1. **Set a session variable from the identity.** FraiseQL resolves configured session
   variables from the request (JWT claims / headers) and injects them as
   transaction-scoped PostgreSQL session variables via `set_config()` before each
   query — see `resolve_session_variables`
   (`crates/fraiseql-core/src/runtime/executor/support/security.rs`). Map e.g.
   `app.tenant_id` to the `tenant_id` claim.
2. **Enforce with RLS.** Define row-level-security policies on the views/tables that
   read that variable with `current_setting('app.tenant_id')`, so PostgreSQL itself
   rejects cross-tenant rows.

Without database-layer RLS, `listTenants` and `listResources` return **every**
tenant's rows to **any** authenticated caller. A worked end-to-end example
(session-var mapping + RLS policy + a two-tenant proof) is tracked in
[#628](https://github.com/fraiseql/fraiseql/issues/628).

## Compiling

```bash
fraiseql compile fraiseql.toml
```

## Scaling

Add more domains:

- `schema/documents/` - Document management
- `schema/workflows/` - Workflow automation
- `schema/analytics/` - Usage analytics
- `schema/audit/` - Audit logging

Each new domain is automatically discovered!

See `../../docs/DOMAIN_ORGANIZATION.md` for details.
