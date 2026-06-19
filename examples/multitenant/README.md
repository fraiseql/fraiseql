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

Multi-tenant safety lives in `fraiseql.toml`, not in the views. The schema
itself is unaware of who is calling — every type carries a `tenantId`
(`organizationId` for `Tenant`), but nothing scopes a query to one tenant on its
own. The `[security]` block does that:

```toml
[security]
default_policy = "authenticated"   # no anonymous access — closes the leak

[[security.rules]]                  # rows scoped to the caller's tenant
rule = "user.tenant_id == object.tenant_id"
```

Without it, `listTenants` and `listResources` return **every** tenant's rows to
**any** caller. Treat the `[security]` block as load-bearing: removing it
re-opens cross-tenant data exposure. The rules above assume the authenticated
principal carries `tenant_id`/`organization_id` claims; map them from your auth
provider accordingly.

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
