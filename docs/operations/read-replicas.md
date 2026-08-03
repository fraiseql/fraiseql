# Read Replicas

FraiseQL can serve compiled GraphQL queries from PostgreSQL streaming replicas
while keeping every write — and everything that *could* write — on the primary
(#407).

## Configuration

```toml
database_url = "postgres://app@primary:5432/appdb"

# Read replicas: compiled queries are served from these, round-robin.
read_replica_urls = [
  "postgres://app@replica1:5432/appdb",
  "postgres://app@replica2:5432/appdb",
]

# Read-your-writes window (ms). After any mutation, reads stay on the primary
# for this long so replication lag cannot serve a client its own stale write.
# Optional; defaults to 5000 when replicas are configured. Setting it without
# read_replica_urls is a configuration error.
read_replica_pin_after_write_ms = 5000
```

Each replica pool inherits the primary pool's sizing (`pool_max_size`,
`pool_timeout_secs`) and the `[database_tls]` transport settings. Replicas are
health-checked at boot: an unreachable replica **refuses startup** rather than
silently shrinking the read fleet, and a server that reports
`pg_is_in_recovery() = false` (a writable server standing in for a standby) is
accepted with a loud warning.

## Routing model

The read/write partition is **static** — a property of the compiled schema, not
of request inspection:

| Surface | Routed to |
|---|---|
| Compiled queries (`SELECT data FROM <view>`), field projections, aggregates, relay pagination, `EXPLAIN` | replica (round-robin) |
| Mutations (`fn_*` function calls, change-log outbox) | primary |
| Raw/administrative SQL (`execute_raw_query`), schema DDL, tenant provisioning, query stats, health checks | primary |
| Auth stores, observers (LISTEN/NOTIFY), CDC | primary (dedicated pools) |

A surface that *can* write is never replica-routed, even when a given statement
happens to be a read.

## Consistency

- **Read-your-writes**: every mutation arms a shared watermark; for
  `read_replica_pin_after_write_ms` afterwards all reads route to the primary.
  The window is an operator assertion about worst-case replica lag — size it
  generously (lag spikes during vacuum, base backups, and failovers).
- Writes that bypass the mutation pipeline (out-of-band jobs, manual SQL) do
  **not** arm the pin; reads racing such writes may see pre-write rows until
  replication catches up.
- Beyond the pin window, replica reads are eventually consistent: stale but
  never torn (each read is a single consistent snapshot on one replica).
- Subscriptions poll through the read path, so replica lag can delay — never
  corrupt — change notifications.

## Failure behaviour

- Boot: any configured replica unreachable → startup refused.
- Runtime: a replica that fails connection acquisition is skipped for that read
  (the next replica is tried, then the primary). Replica loss degrades read
  capacity, never read availability.

## Tenant isolation

Replica pools are built from the same pool configuration as the primary,
including the per-tenant `search_path` lowered into the PostgreSQL startup
`options` (#809). A schema-isolated tenant's reads resolve against its schema on
every pool, verified by an integration test that inspects
`pg_settings.reset_val` through a replica-routed connection.

## Limitations

- **Per-tenant pools are primary-only.** A tenant registration supplies one
  connection string; `read_replica_urls` describes the server's primary
  database. Extending replicas to tenant pools needs per-tenant replica URLs on
  the registration API.
- **The wire backend refuses replica configuration** at boot (no replica
  routing exists there; accepting the config would silently serve every read
  from the primary).
- No replication-lag measurement (`max_lag_ms`), no per-query routing
  overrides, no geographic selection — see the tracking issue for those.
