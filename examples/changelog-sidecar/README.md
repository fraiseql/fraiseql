# changelog-sidecar

A minimal pull-based consumer of the FraiseQL entity change log over GraphQL
(issue #149). It polls `entity_change_logs` with a keyset cursor, processes each
batch, and advances a per-consumer checkpoint via `upsert_transport_checkpoint`
— all over the same GraphQL endpoint as the rest of your API, with no
side-channel PostgreSQL connection.

See the guide: [`docs/guides/changelog-graphql.md`](../../docs/guides/changelog-graphql.md).

## Server config

The FraiseQL server this talks to must opt in:

```toml
# fraiseql.toml
[observers]
enabled = true

[changelog]
expose     = true
read_role  = "changelog_reader"
write_role = "changelog_writer"
```

The sidecar's auth token must carry the `changelog_reader` and
`changelog_writer` roles.

## Run

```bash
export FRAISEQL_URL=http://localhost:8000/graphql
export FRAISEQL_TOKEN=<jwt-with-changelog-roles>
export CONSUMER_NAME=sidecar-1        # checkpoint transport_name

deno run --allow-net --allow-env sidecar.ts
# or: node --experimental-strip-types sidecar.ts
```

Inject test events by running mutations against your schema (any mutation that
writes `tb_entity_change_log` via the observer system), then watch the sidecar
print each `pk_entity_change_log` it processes and the checkpoint it advances to.

## What to look for

- The sidecar resumes from the stored checkpoint on restart (no reprocessing).
- Pagination is gap-free and numeric — `pk 9 → 10`, never lexicographic `9 → 90`.
- Idle polling backs off (`sleep`) when there are no new events.
