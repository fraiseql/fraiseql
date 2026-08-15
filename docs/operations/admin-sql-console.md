# The admin SQL console

`POST /api/v1/admin/sql` runs a statement you typed against the configured
database. It is the only endpoint on the server that executes SQL FraiseQL did
not generate, and it is off in three independent ways until you turn all three
on. This page is about what it does, what bounds it, and — just as importantly —
what it does **not** bound.

If you only need to *look* at data, you probably do not need this. The Studio Data
tab reads entities through the compiled schema, and `POST /api/v1/admin/explain`
plans a named query. Both stay inside the surface the schema describes. Reach for
the console when you need something the schema does not model: an ad-hoc join
during an incident, a one-off correction, a "what would this migration do?"
preview.

## Turning it on

```toml
admin_api_enabled = true
admin_token = "…at least 32 characters…"
admin_readonly_token = "…a different 32+ characters…"   # optional but recommended

[admin_sql]
enabled = true
statement_timeout_ms = 30000   # default
max_rows = 1000                # default
allow_commit = true            # default
```

and build the server with the feature:

```bash
cargo build --release -p fraiseql-server --features admin-sql
```

Each missing piece is a **boot error naming itself**, not a missing route:

| Missing | What the server says at boot |
|---|---|
| the `admin-sql` cargo feature | `[admin_sql] enabled = true requires the 'admin-sql' cargo feature…` |
| `admin_api_enabled` | `[admin_sql] enabled = true requires admin_api_enabled = true…` |
| `admin_token` | `admin_api_enabled is true but admin_token is not set…` |

That is deliberate. A console you configured and cannot reach is
indistinguishable from a wrong URL; a server that refuses to start tells you
which piece is missing.

The server also logs a `WARN` line at every boot where the console is mounted,
naming the bounds in force. An endpoint this powerful should be visible in the
startup log of a host you did not configure yourself.

## The two tokens

| Credential | Transaction | May commit |
|---|---|---|
| `admin_readonly_token` | `READ ONLY` | no |
| `admin_token` | read-write | on request |

**Read-only is the transaction's mode, not an inspection of your SQL.** PostgreSQL
refuses the write with SQLSTATE `25006` regardless of how it is spelled, which
matters because plenty of writes do not look like writes: `WITH x AS (UPDATE …)
SELECT …`, `SELECT nextval('s')`, a `VOLATILE` function that writes. A regex over
the statement text would pass all three.

A `commit: true` request under the read-only token is refused before the database
is touched, rather than being allowed to "succeed": `COMMIT` on a `READ ONLY`
transaction returns fine and persists nothing, and an endpoint answering
`committed: true` over a change that never happened is worse than one that says
no.

In **single-token mode** (`admin_readonly_token` unset) there is no read-only
credential at all — `admin_token` grants everything, exactly as it does for the
rest of the admin API. Set the second token.

## Rollback by default

```jsonc
// POST /api/v1/admin/sql
{
  "sql": "UPDATE tb_order SET status = 'cancelled' WHERE id = 4711 RETURNING *",
  "commit": false          // the default — you may omit it
}
```

The statement runs. Its `RETURNING` row comes back. Then the transaction rolls
back and nothing persists. That is what makes the default useful: you can see
what a correction *would* do before doing it.

Send `commit: true` to keep it. The response reports `committed`, so a preview and
a change are never confused for one another in a log or a screenshot.

Set `allow_commit = false` in the section to remove the opt-in entirely; the
console then refuses every commit by name and is strictly a preview tool.

## Bounds

| Bound | Mechanism | Configurable per request |
|---|---|---|
| statement timeout | `SET LOCAL statement_timeout` | yes, **downward only** |
| row cap | the read stops and reports `truncated` | yes, **downward only** |
| one statement | the extended query protocol's Parse | no |

A request may tighten a bound and never loosen it, and the response reports the
values **actually applied** — `statement_timeout_ms` and `max_rows` come back on
every answer. An operator who asked for ten minutes on a server capped at thirty
seconds needs to see thirty seconds, or they will read the cancellation as a hung
database.

`0` is refused rather than clamped, for both bounds. PostgreSQL reads
`statement_timeout = 0` as *no* timeout, so accepting it would turn the
strictest-looking request into the one with no limit at all.

**One statement per request** is not a `split(';')` — a semicolon inside a string
literal is not a statement boundary, and any parser that thinks it is has a
bypass. It is the wire protocol: FraiseQL sends the statement with the extended
query protocol, which parses exactly one command. `SELECT 1; DROP TABLE …` is
rejected by PostgreSQL before anything runs, and `; COMMIT` cannot be appended to
escape rollback-by-default.

## Previewing another identity's view

```jsonc
{
  "sql": "SELECT * FROM v_invoice",
  "impersonate": {
    "user_id": "0f3c…",
    "tenant_id": "11111111-1111-1111-1111-111111111111",
    "roles": ["billing"],
    "claims": { "region": "eu-west" }
  }
}
```

`impersonate` sets the transaction-local session variables your compiled schema's
`[session_variables]` mappings would produce for that identity — through the same
function the executor calls on a real query, so a preview cannot differ from the
thing it previews. RLS policies reading `current_setting('app.tenant_id')` then
apply as they would for that user.

Two consequences worth internalising:

- **Without `impersonate`, nothing is set.** The statement runs as the connection pool's role with
  no session variables, which for a policy comparing against `current_setting(…, true)` means
  *no rows*, not *all rows*. That is the "as admin" case and it is the default.
- **A schema with no `session_variables` mappings has no identity to preview.** Impersonating on
  one sets nothing, correctly — there is no RLS identity in play, and pretending otherwise would
  show you a filtered view the database is not applying.

Claims in the reserved `fraiseql.` namespace (`fraiseql.actor_type`,
`fraiseql.transport`, `fraiseql.acting_for`) are **refused by name**. The token
extractor strips that namespace from real tokens precisely so a client cannot
write the values the server derives; an operator endpoint that accepted them
would be the hole the extractor exists to close.

## Every execution is audited

One entry per request, in the same tamper-evident chain as the credential events:

```
event_type   = admin_sql_execution
secret_type  = admin_token
subject      = admin_token | admin_readonly_token
success      = true | false
context      = peer_ip=… sha256=… commit_requested=… committed=… impersonate=… sql=…
```

Recorded for **every** request that reaches the endpoint — executed, failed, and
refused. The refused `DROP` and the mistyped `UPDATE` are the entries an
investigation starts from; a ledger holding only what worked describes a different
session than the one that happened.

The statement text is truncated to about 1 KB (the audit entry is bounded), so the
SHA-256 of the whole text is recorded alongside it. A truncated record still
identifies exactly which statement ran.

Wire a database-backed audit logger before enabling the console in production. The
default logger writes structured `tracing` records, which is fine if your log
pipeline retains them and is not if it does not.

## What this does not bound

Stated plainly, because the controls above are easy to over-read:

- **A committed statement is a real change with no schema-level validation.** No mutation pipeline,
  no field authorization, no change-log row, no observers, no cache invalidation. If you commit a
  write that changes rows a cached query describes, follow it with `POST
  /api/v1/admin/cache/clear`.
- **The console runs on the primary, always.** It holds a pooled connection for the life of the
  request, and an open statement takes an `ACCESS SHARE` lock on what it reads — a long console
  query can block a DDL migration.
- **The database role is your outer boundary.** Every bound here applies to the transaction
  FraiseQL opens; none of them constrains what the pool's role is *allowed* to do. If that role
  can `DROP SCHEMA`, so can `admin_token`. Grant the application role only what the application
  needs, and the console inherits that.
- **`admin_token` is the whole gate.** There is no per-statement approval, no second factor and no
  allow-list. Treat it as a production database credential, because with this endpoint mounted
  that is exactly what it is.

## Response shape

```jsonc
{
  "status": "success",
  "data": {
    "columns": ["id", "status"],
    "rows": [[4711, "cancelled"]],
    "truncated": false,
    "rows_affected": 1,          // omitted when the read stopped early
    "committed": false,
    "read_only": false,
    "statement_timeout_ms": 30000,
    "max_rows": 1000
  }
}
```

Rows are **positional**, aligned to `columns`, rather than keyed by name:
`SELECT 1 AS a, 2 AS a` is legal SQL, and a name-keyed object would silently drop
one of the two.

| Status | Meaning |
|---|---|
| `400` | empty statement, a bound requested as `0`, or a reserved-namespace claim |
| `401` | no `Authorization` header |
| `403` | wrong token; a write under the read-only token; a refused commit |
| `408` | the statement timeout cancelled it |
| `429` | too many failed authentication attempts from this peer |
| `500` | anything else PostgreSQL rejected — its own message is passed through |

The database's error text is returned deliberately. The endpoint is reachable only
with an admin credential and its entire purpose is running SQL you wrote;
withholding the reason it failed would make it useless. Only PostgreSQL's
`message` is forwarded, never `DETAIL` — which is where row values appear.
