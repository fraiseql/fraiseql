---
title: Entity change log over GraphQL
description: Poll-based event consumption via the FraiseQL GraphQL endpoint, without a separate database connection.
sidebar_position: 18
---

<!-- The Docusaurus front-matter `title:` field is the canonical H1; the
     body intentionally starts with prose to keep markdownlint MD025 happy.
     If you add an H1 here, set the front-matter `title:` to empty first. -->

The observer system writes every entity mutation to `core.tb_entity_change_log`.
When `[changelog] expose = true`, FraiseQL surfaces that log as queryable GraphQL
types — so a sidecar consumer (AI scoring, search-index sync, audit dashboard)
talks to the same GraphQL endpoint as the rest of your stack instead of opening a
side-channel PostgreSQL connection. Same auth, same audit logging, same rate
limiting, same RBAC.

## Why pull, not push

The pull pattern — poll the changelog, process a batch, advance a cursor — is
operationally simpler than push webhooks:

- **Crash recovery**: resume from the stored cursor; no dead-letter queue.
- **Ordering**: guaranteed by the monotonic `pk_entity_change_log` key.
- **Backpressure**: the consumer controls batch size via `limit`.
- **Reprocessing**: rewind the cursor to replay.

This feature *exposes* that pattern; it is not a queue or a push delivery system.

## Prerequisites

The exposed views read from tables the observer system supplies — FraiseQL does
**not** create them for you:

- `core.tb_entity_change_log` — the change-log table (observer/install convention).
- `core.tb_transport_checkpoint` — installed by the observer NATS-transport migration.
- `app.mutation_response` — the standard FraiseQL mutation-result composite
  (see [mutation-response.md](../architecture/mutation-response.md)).

`[observers]` must be enabled; the compiler rejects `[changelog] expose = true`
otherwise.

## Enable it

```toml
# fraiseql.toml

[observers]
enabled = true            # required — the changelog tables live here

[changelog]
expose     = true
schema     = "core"               # default; PG schema holding the tables
read_role  = "changelog_reader"   # default; null disables the read gate
write_role = "changelog_writer"   # default; null disables the write gate
max_limit  = 1000                 # default; documented page ceiling
```

On the next `fraiseql compile`, the changelog migration installs:

- `core.v_entity_change_log`
- `core.v_transport_checkpoint`
- `core.fn_upsert_transport_checkpoint`

(see `crates/fraiseql-observers/migrations/07_create_changelog_views.sql`)

…and the schema gains three operations, gated by the configured roles:

- `entity_change_logs` — cursor-paginated change-log query (needs `read_role`).
- `transport_checkpoint(transport_name)` — fetch one consumer's cursor (needs `read_role`).
- `upsert_transport_checkpoint(transport_name, last_pk)` — advance a cursor (needs `write_role`).

Callers without the role see `"not found in schema"` (not `FORBIDDEN`) — the
operations are hidden behind the role gate to prevent enumeration.

## Cursor pagination

`entity_change_logs` is a standard FraiseQL list query: it uses the generic
`where` / `orderBy` / `limit` filter machinery, identical to any user-authored
list type. Page by keyset on the monotonic `pk_entity_change_log`:

```graphql
query Page($cursor: Int!, $limit: Int!) {
  entity_change_logs(
    where:   { pk_entity_change_log: { gt: $cursor } }
    orderBy: { pk_entity_change_log: "ASC" }
    limit:   $limit
  ) {
    pk_entity_change_log
    id
    object_type
    object_id
    modification_type
    object_data
    created_at
  }
}
```

`pk_entity_change_log` is an `Int`, so `gt` and `orderBy` compare numerically —
pagination is gap-free even as new rows arrive mid-scan. Narrow the stream with
the same filter object:

```graphql
where: { pk_entity_change_log: { gt: $cursor }, object_type: { eq: "User" } }
```

Keep `limit` at or below the configured `max_limit` (default 1000).

## Checkpoints

Persist each consumer's position by `transport_name`:

```graphql
query Cursor($name: String!) {
  transport_checkpoint(transport_name: $name) { last_pk }
}

mutation Advance($name: String!, $pk: Int!) {
  upsert_transport_checkpoint(transport_name: $name, last_pk: $pk) {
    transport_name
    last_pk
  }
}
```

`upsert_transport_checkpoint` is idempotent: re-advancing to the same `last_pk`
is a no-op.

## Consumer loop

```typescript
const NAME = "sidecar-1";
while (true) {
  const cur = await client.query("Cursor", { name: NAME });
  const cursor = cur?.transport_checkpoint?.last_pk ?? 0;

  const page = await client.query("Page", { cursor, limit: 100 });
  const events = page.entity_change_logs;
  if (events.length === 0) { await sleep(1000); continue; }

  for (const e of events) await handle(e);

  await client.mutate("Advance", {
    name: NAME,
    pk: events[events.length - 1].pk_entity_change_log,
  });
}
```

A runnable version ships in [`examples/changelog-sidecar/`](https://github.com/fraiseql/fraiseql/tree/main/examples/changelog-sidecar).

## Security considerations

- The `read_role` sees **everything** mutations have written. Grant it only to
  operator-level users or trusted sidecars.
- `object_data` is the Debezium-style envelope and may contain PII (before/after
  row payloads). Apply field-level encryption at the source table if that is a
  concern.
- RLS does **not** apply to the changelog views by default — they see every row.
  For per-tenant changelog access, build a tenant-scoped view on top of
  `core.v_entity_change_log` in your own schema and point `[changelog] schema` at it.

## Retention

`tb_entity_change_log` grows unbounded with every mutation. There is no built-in
TTL today; if your write volume is high, plan a cleanup job (a periodic
`DELETE FROM core.tb_entity_change_log WHERE created_at < NOW() - INTERVAL '30 days'`,
or a `pg_partman` partition on `created_at`). Exposing the log over GraphQL does
not change this. Tracked as a follow-up.
