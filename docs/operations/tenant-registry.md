# Tenant and domain registration

With `[tenancy.runtime] enabled = true`, the server routes each request to a per-tenant
executor. The tenants and custom domains it knows are registered at run time through the
admin API, and **they are held in the server process's memory only**. Nothing writes
them to the database, and nothing reloads them at boot. Persisting them is tracked in
#1637; until then, whatever registers your tenants must register them again after
every start, on every replica.

## The routes

The write routes take `admin_token`. The read routes take `admin_readonly_token` when it
is set, and `admin_token` otherwise:

| Route | Effect |
|---|---|
| `PUT /api/v1/admin/tenants/{key}` | Register a tenant, or replace its registration: schema, connection, quota and budgets |
| `DELETE /api/v1/admin/tenants/{key}` | Remove the tenant |
| `POST /api/v1/admin/tenants/{key}/suspend`, `…/resume` | Refuse the tenant's requests (503 with `Retry-After`), or serve them again |
| `PUT /api/v1/admin/domains/{domain}` | Map a `Host` to a tenant key: `{"tenant_key": "acme"}` |
| `DELETE /api/v1/admin/domains/{domain}` | Remove the mapping |
| `GET /api/v1/admin/tenants`, `…/{key}`, `…/{key}/health`, `…/{key}/events` | Read the registry and the tenant lifecycle events |
| `GET /api/v1/admin/domains` | Read the domain mappings |

The request bodies are described with the features that use them:
[tenant storage quota](tenant-storage-quota.md), [actor cost budgets](actor-policies.md),
[read replicas](read-replicas.md).

## What a restart loses

The server starts with an empty registry, holding only the default executor, and an empty
domain map. After a restart, or a crash, or a rolling deploy:

- every tenant registered with `PUT /api/v1/admin/tenants/{key}` is gone, and so is its
  suspended state, quota and budget;
- every domain mapping is gone;
- the lifecycle events returned by `…/{key}/events` are gone.

Until a tenant is registered again, a request that names it (by its token's tenant claim
or by `X-Tenant-ID`) is refused as not authorized: the tenant is not registered. A request that
names no tenant, including an anonymous one whose `Host` is no longer mapped, is served by
the default executor, as it would be for any domain the server does not know.

## More than one replica

Each server process has its own registry and its own domain map, and nothing keeps them in
step. A `PUT` sent through a load balancer registers the tenant on the one replica that
received it; the others still refuse the tenant, or serve its domain with the default
executor. The same holds for `DELETE`, `suspend` and `resume`, and each replica's
`…/events` lists only the operations it received.

Address every replica's admin API directly, not through the load balancer, and treat the
registrations as configuration your own provisioning owns: keep the list outside FraiseQL,
and apply it to each replica when it starts and whenever it changes. A replica that serves
traffic before it has been given the list refuses those tenants' requests in the meantime.
