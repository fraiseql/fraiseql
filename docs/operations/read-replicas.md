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

# Bounded staleness (ms). A replica whose measured replay lag exceeds this is
# skipped and the read is served by the primary. Optional; unset means no
# lag-based routing at all.
read_replica_max_lag_ms = 2000

# How often each replica is probed for replay lag and recovery state. Optional;
# defaults to 1000. Probing runs whether or not max_lag is set — it is also what
# detects a replica a failover promoted after boot. Must be strictly smaller
# than read_replica_max_lag_ms.
read_replica_health_probe_interval_ms = 500
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

### Per-query overrides

The partition above is structural. `read_routing` on a compiled query is the one
place a schema author can say something the structure cannot:

| `read_routing` | Replicas | Read-your-writes pin | Result cache | Staleness budget |
|---|---|---|---|---|
| `any` (default) | eligible | applies | may serve | the server's |
| `primary` | never | applies | **bypassed** | n/a |
| `replica` | eligible | **ignored** | may serve | its own `max_lag_ms`, else the server's |

`primary` bypasses the result cache deliberately. It is asked for when staleness
is a correctness problem, and a cache hit is stale data by construction — a query
refused a stale replica and then served a stale cache entry would have got the
opposite of what it asked for through a different door. `primary` means *fresh*,
not *from a particular host*.

`replica` is the reverse trade: the query declares it is not the kind of read the
pin exists to protect (feeds, dashboards, exports), so paying primary capacity
for freshness nobody needs is waste worth naming. It still falls back to the
primary when no replica qualifies — refusing to answer would turn a capacity
preference into an outage.

FraiseQL defines and enforces this shape; an authoring language emits it. SpecQL's
`@reads_from(...)` directive (evoludigit/specql#13) is one spelling of it. Replica
**topology** deliberately stays out of the compiled artifact — URLs are server
configuration and secrets, not schema.

## Consistency

- **Read-your-writes**: every mutation arms a shared watermark; for
  `read_replica_pin_after_write_ms` afterwards all reads route to the primary.
  The window is an operator assertion about worst-case replica lag — size it
  generously (lag spikes during vacuum, base backups, and failovers).
- **Bounded staleness** turns that assertion into a measurement, and covers
  everyone *else's* writes rather than the client's own. A background probe reads
  each replica's replay lag; a replica is eligible while

  ```
  lag_at_last_probe + age_of_that_probe  <=  read_replica_max_lag_ms
  ```

  That sum is a true upper bound, not an estimate: replay lag grows at most one
  millisecond per millisecond of wall clock, so however stale the probe is, the
  lag cannot have grown by more than its age. It also makes the gate self-closing
  — nothing has to notice that probing stopped, because an unrefreshed probe ages
  past any budget on its own and the replica falls out of rotation.

  A replica whose lag cannot be *measured* is never eligible while a budget is
  set: never probed, probe failing, no transaction replayed yet, or promoted out
  of recovery. Unknown staleness is not zero staleness, and the primary is always
  a correct answer.
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
- **Failover**: a replica that booted as a standby and is later found outside
  recovery has been promoted, and stops taking reads — whether or not a staleness
  budget is configured. A promoted server accepts writes of its own, so reads
  from it diverge from the primary in *both* directions, which is not a staleness
  any budget could bound. The boot health check runs once and structurally cannot
  see this; the periodic probe is what does.

  A replica that booted *outside* recovery keeps the acceptance the boot check
  gave it, with the loud warning it already logs — dev rigs legitimately stand a
  plain database in for a standby, and no dev rig transitions.

## Tenant isolation

Replica pools are built from the same pool configuration as the primary,
including the per-tenant `search_path` lowered into the PostgreSQL startup
`options` (#809). A schema-isolated tenant's reads resolve against its schema on
every pool, verified by an integration test that inspects
`pg_settings.reset_val` through a replica-routed connection.

Tenants may register replicas of their own:

```json
POST /admin/tenants/acme
{
  "connection": {
    "connection_string": "postgres://app@acme-primary/appdb",
    "read_replica_urls": ["postgres://app@acme-replica/appdb"]
  }
}
```

The registration names the *topology*; the server names the *policy*. Pin window,
staleness budget and probe cadence are stamped onto every tenant pool from the
server's own settings, exactly as `[database_tls]` is (#801) — a registration body
that could send its own `max_lag_ms` would be deciding how stale its reads may be
against a server whose operator already decided.

## Limitations

- **The wire backend refuses replica configuration** at boot (no replica
  routing exists there; accepting the config would silently serve every read
  from the primary).
- No geographic / latency-aware selection — see the tracking issue.
