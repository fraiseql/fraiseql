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
