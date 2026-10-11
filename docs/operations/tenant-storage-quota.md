# Tenant storage quota

A tenant registered with `max_storage_bytes` has its storage measured, and while it is
over the quota its writes are refused. Its reads are served throughout.

```jsonc
// PUT /api/v1/admin/tenants/{key}
{
  "schema": { /* the tenant's compiled schema, in schema-mode tenancy */ },
  "connection": { "connection_string": "postgres://…" },
  "max_storage_bytes": 5368709120   // 5 GiB
}
```

```toml
# fraiseql.toml (server)
[tenancy.storage_quota]
interval_secs = 300   # how often every tenant with a quota is measured; 0 is refused
```

## What is measured

The tenant's own schema (`tenant_<key>`, the schema its `search_path` names), on the
tenant's own connection: every table and materialized view in it, with their indexes and
TOAST (`pg_total_relation_size`). A tenant is measured when it is registered, every tenant
when the server starts, then one every `interval_secs`. A measurement that takes longer than
30 seconds is abandoned with a warning and the tenant keeps its last state, so one
unresponsive database never holds up the others.

A quota needs **schema-mode tenancy**. A row-mode tenant shares its tables with every
other tenant, so there is nothing of its own to measure: registering one with
`max_storage_bytes` is refused.

## What is refused

Once a measurement finds the tenant over its quota, every write it makes is refused
before anything of it runs, on every transport: GraphQL, REST, MCP, async operations and
the writes of dispatched functions. The answer is `507 Insufficient Storage` with the code
`INSUFFICIENT_STORAGE`, naming the measured usage and the quota. Nothing is written, no
`before:mutation` hook runs.

A GraphQL operation with several mutation roots answers each refused root inside its
response (`data` `null` for that root, an `errors` entry), as it does for any per-root
error; the 507 status applies to a single-root operation.

The refusal lifts on its own at the first measurement that finds the tenant back under.
A tenant over its quota cannot free space through the API, since a delete is a write and is
refused like any other. The operator either frees space in the tenant's schema directly
(space PostgreSQL has actually reclaimed: `TRUNCATE`, `VACUUM FULL`, dropped relations) or
raises the quota by registering the tenant again, which measures it at once.

## Observing it

`/metrics` carries `fraiseql_tenant_storage_bytes{tenant}` and
`fraiseql_tenant_storage_quota_bytes{tenant}`. The server logs each change of state, over
and back under, with the usage and the quota.

The old `max_storage_bytes_advisory` key, from when nothing enforced the value, is
refused at registration.
