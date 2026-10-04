# Tenant administrators

A multi-tenant deployment has two kinds of administrator:

| Principal | Credential | Administers |
| --- | --- | --- |
| **Platform** | the deployment `admin_token` | every tenant, and the platform's own rows |
| **Tenant** | a tenant admin token | exactly one tenant |

A tenant admin token is confined to its tenant by the credential itself. Nothing in a request
can widen it.

## Issuing a tenant admin token

Only the platform issues, lists and revokes tenant admin tokens:

```bash
# Mint one. The token is shown exactly once; only sha256(token) is stored.
curl -X POST https://api.example.com/api/admin-tokens \
  -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  -d '{"tenant_id": "<tenant uuid>", "description": "Acme IT"}'

# List them (optionally ?tenant_id=…), and revoke one
curl https://api.example.com/api/admin-tokens -H "Authorization: Bearer $ADMIN_TOKEN"
curl -X DELETE https://api.example.com/api/admin-tokens/<id> -H "Authorization: Bearer $ADMIN_TOKEN"
```

- `tenant_id` is required. A tenant admin token with no tenant would be a second
  deployment-wide credential.
- Tokens start with `fraiseql_ta_`, so a leaked one is easy to recognise.
- The table is `core.tb_admin_token`. It is created at boot when `admin_token` is set and the
  server has a database pool.

## What a tenant administrator can do

A tenant administrator uses the same routes as the platform, with the same bearer header:

| Route | Tenant administrator |
| --- | --- |
| `/api/saml/idps` | its own tenant's IdPs |
| `/api/scim/tokens` | its own tenant's SCIM provisioning tokens |
| `/api/roles`, `/api/user-roles`, `/api/audit/permissions` | its own tenant's roles, assignments and audit rows |
| `/api/permissions` | read only: the permission catalogue is shared by every tenant |

The same three rules apply on every one of these routes:

1. **Naming no tenant means its own.** A role, IdP or SCIM token a tenant administrator creates
   belongs to its tenant, and a list shows only its tenant. Naming no tenant never means
   "every tenant".
2. **Naming another tenant is `403`.**
3. **Another tenant's row is `404`, exactly like a missing one.** This holds for platform
   (global) rows too, so the response cannot be used to probe for identifiers.

IdP names form one namespace across tenants, because the name is the account provider key.
Creating a name that another tenant already holds is a `409`, like any other taken name.

A role can be held only in its own tenant, for every administrator. Assigning tenant A's role
in tenant B is `400 tenant_mismatch`, while a global role may be assigned in any tenant.

## What stays platform-only

Every other admin surface accepts only the deployment `admin_token`. A tenant admin token
presented there is answered as the wrong token it is (`403`). These surfaces are:

- `/api/admin-tokens`
- API-key management (`/api/v1/admin/api-keys`)
- the identity-cache flush
- email suppression
- everything under `admin_api_enabled`: Studio `/admin/v1/*`, and `/api/v1/admin/*`, which
  covers the tenant registry, domains, cache, query statistics, the SQL console and storage
  policies.

A router accepts tenant administrators only by mounting behind the tenant-aware gate. A router
added later is therefore platform-only until it opts in, and never inherits tenant access by
accident.
