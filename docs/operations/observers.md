# Operating Observers under fraiseql-server

Observers run inside the `fraiseql-server` process. There is no separate observer
binary, CLI or container image: you operate observers through `fraiseql.toml`, the
server's environment, its admin HTTP API, its logs and its `/metrics` endpoint.

---

## Where Observers Run

The observer runtime is compiled into `fraiseql-server` by Cargo features:

| Feature | Adds |
|---------|------|
| `observers` | The runtime (change-log listener, matcher, executor, in-memory DLQ) and the admin HTTP API |
| `observers-nats` | The NATS JetStream transport |
| `observers-cache` | The Redis backend for `cache` actions |
| `observers-metrics` | `fraiseql_observer_*` series on `/metrics` |
| `observers-enterprise` | `observers-metrics` + `observers-nats` + the library's `enterprise` feature |

Configuration lives in `fraiseql.toml`. `[observers]` is shared with the compiler
(`enabled`, `backend`, `nats_url`; `[[observers.handlers]]` is refused: observers are
declared in the authoring schema or added through the admin API, see
[Declaring observers in the schema](#declaring-observers-in-the-schema)). `backend` selects the event transport: `"postgres"` (the
default) or `"nats"`, with `nats_url` as the broker URL. `backend = "redis"` and
`redis_url` are refused at compile: the runtime has no Redis transport. The compiler reads it in both workflows: from a TOML
schema, and from the `fraiseql.toml` beside an SDK's `schema.json`. A `schema.json` that
carries its own `observers_config` must agree with `[observers]`; the compile refuses a
disagreement and names the key. The server's own tuning lives under
`[observers.runtime]` (`poll_interval_ms`, `batch_size`, `channel_capacity`,
`auto_reload`, `reload_interval_secs`, `max_dlq_size`, `log_payloads`) with the
sub-tables `[observers.runtime.transport]`, `[observers.runtime.email]`,
`[observers.runtime.pool]` and `[observers.runtime.redis]`. `[observers.runtime]`
rejects unknown keys, so a typo stops the server at boot instead of being ignored.
`batch_size` (default 100) must be between 1 and 10000: 0 would read no change-log
row, and an observer checkpoint records at most 10000 rows per batch. A value outside
that range stops the server at boot.

Each replica loads `core.tb_observer` at start. With `auto_reload = true` (the default) it
reloads every `reload_interval_secs` (default 60), so an observer created, edited or
disabled through any replica's admin API takes effect on every replica within one
interval. `POST /api/observers/runtime/reload` applies it at once, but only on the
replica that serves the request. With `auto_reload = false`, that endpoint is the only
way to reload. `reload_interval_secs = 0` with `auto_reload = true` is refused at boot.
An enabled runtime that loads no observers logs a warning, at start and on every reload
that still finds none.

### Declaring observers in the schema

An observer declared in the authoring schema (the SDK's observer decorator or builder,
the `observers` array of `schema.json`) is compiled into `schema.compiled.json`. At boot,
before the runtime loads `core.tb_observer`, the server writes each declared observer
there. No SQL seed and no call to the admin API is needed: deploying the schema is what
installs its observers.

```json
{
  "name": "order_created",
  "entity": "Order",
  "event": "INSERT",
  "condition": "total > 100",
  "actions": [
    {
      "type": "webhook",
      "url_env": "ORDER_HOOK_URL",
      "signing_secret_env": "ORDER_HOOK_KEY",
      "body_template": "{\"order\": {{ id }}, \"total\": {{ total }}}"
    }
  ],
  "retry": {
    "max_attempts": 3,
    "backoff_strategy": "exponential",
    "initial_delay_ms": 1000,
    "max_delay_ms": 60000
  }
}
```

**Retry.** `retry` may be left out, and so may any of its fields. A field left out takes
the policy the runtime runs when it is given none: 3 for `max_attempts`, `exponential` for
`backoff_strategy`, 1000 for `initial_delay_ms`, 60000 for `max_delay_ms`. It is the rule
an observer created through the admin API follows (see
[Observer Management](#observer-management-changelog-and-checkpoints)). The compiler fills
the fields in, so `schema.compiled.json` carries the whole policy. An SDK writes `retry`
only when the author sets one.

**What the compile checks.** A declaration is refused, naming the observer and the key,
when the runtime could not run it as written:

- `[observers] enabled` is not `true`: the server would start no runtime.
- An action is not one the runtime dispatches, lacks a key the runtime requires (a
  webhook needs `url` or `url_env`), or carries a key the runtime does not read. The
  runtime ignores a key it does not know, so `message` on a Slack action (the key is
  `message_template`) would otherwise be stored and deliver nothing. An email action's
  `to_template`, `subject_template` and `reply_to` are refused for the same reason: the
  dispatcher sends to `to` with `subject` and uses none of the three. The admin API
  applies this same check to the actions it is sent (see
  [Observer Management](#observer-management-changelog-and-checkpoints)).
- A webhook carries a `signing_secret` literal. The compiled schema is a build artifact;
  name the environment variable that holds the key with `signing_secret_env`.
- A webhook `headers` entry whose name says it carries a credential: the name contains
  `authorization`, `api-key`, `cookie`, `password`, `secret` or `token`, whatever its case
  (`Authorization`, `X-API-Key`, `X-CSRF-Token`). Name the environment variable that holds
  its value under `headers_env` instead.
- A `headers_env` entry that is not an HTTP header name or names no variable, or a header
  named in both `headers` and `headers_env` (header names compare case-insensitively).
- A webhook `body_template` names a field the entity does not hold. A placeholder names a
  field by the key the stored document holds it under: `{{ review_status }}` for a field
  published as `reviewStatus`. A placeholder that reads a field the type embeds
  (`{{ customer.name }}`) compiles with a warning when a table of the type is captured
  as its row: see
  [External-write capture](../architecture/external-write-capture.md#a-table-that-keeps-its-entity-document).
- The `condition` does not parse, or `retry` is not a policy the runtime reads. The
  policy is checked with the fields left out filled in, so a `max_delay_ms` below the
  default `initial_delay_ms` is refused as it would be written out.
- The `condition` reads a key no event of the entity holds. A condition reads one
  top-level key of the event, by the key the stored document holds it under:
  `review_status == 'approved'` for a field published as `reviewStatus`. A dotted key in
  a call (`has_field('customer.email')`) is refused, since no event holds one. A key that
  names no field of the type is refused unless a table of the type is captured as its
  row, where it may be a column: it then compiles with a warning, as a key the type
  embeds does. See
  [External-write capture](../architecture/external-write-capture.md#a-table-that-keeps-its-entity-document)
  for why conditions are more permissive there than templates.
- `fraiseql.toml` writes no `[project] name`. The name is the owner the observers are
  stored under (see below), so a schema that declares one needs it.

**Header values from the environment.** `headers` values are stored as written, in the
compiled schema and in `core.tb_observer`. `headers_env` maps a header name to the name of
an environment variable, which the server reads each time the webhook is delivered, so
only the variable's name is stored:

```json
{
  "type": "webhook",
  "url_env": "ORDER_HOOK_URL",
  "headers": {"X-Tenant-Id": "acme"},
  "headers_env": {"Authorization": "ORDER_HOOK_TOKEN"}
}
```

**The variables a declared observer names are checked at boot.** A server's environment
does not change once it is running, so a variable that is not set, or is empty, at boot
would fail every delivery that reads it. In production the server refuses to start when a
declared observer's `url_env`, `signing_secret_env`, `headers_env` or Slack
`webhook_url_env` names such a variable. The refusal lists each one by observer, action,
key and variable name, never a value. A `url_env` beside a literal `url` is not read, so
it is not checked. In development the server starts and logs a `WARN` for each one.
A changed value takes effect when the server restarts.

An observer created through the admin API is not checked at boot: a stored row never
stops the server. A variable it names that is not set, or is empty, when a webhook is
delivered fails that delivery: nothing is sent, and the event is dead-lettered. The admin
API accepts `headers_env` too, and shows the variable names in its responses; it still
accepts a literal value under `headers`, which its responses mask.

**What the boot writes.** A declared observer is a global row (`fk_customer_org` null)
whose `managed_by` is `schema:<[project] name>`, for example `schema:billing`:

| The schema | `core.tb_observer` at boot |
|------------|----------------------------|
| declares an observer no row holds | a new row |
| declares an observer whose row differs | the same row, rewritten (its id and its execution log stay) |
| declares a name a global row from the admin API or a seed script already holds | that row is adopted: rewritten from the declaration and marked with the project's owner. The server logs a `WARN` naming it |
| no longer declares an observer it once did | the row is soft-deleted. The server logs a `WARN` naming the observer and its owner: "removed because the current schema no longer declares it" |

An organisation's observers are never read or written by this, whatever their names.
The write is one transaction under an advisory lock, so replicas booting together store
one set. During a rolling deploy the last replica to boot decides what is stored, and
replicas of the earlier release pick it up on their next reload.

Several projects can declare observers on one database. Each project's are stored under
its own name, and a boot writes and removes only its own project's rows.

- **One observer name, two projects.** A name is held by one project. The boot of a second
  project that declares it is refused, naming both projects, and writes nothing. Rename
  one of the two observers.
- **One project name, two schemas.** Two schemas compiled with the same `[project] name`
  are one owner. Each boot then removes the observers the other declares, exactly as a
  redeploy of one project with a changed schema does; the server cannot tell the two
  apart. The `WARN` above is where it shows. Give each schema that declares observers on
  the database its own `[project] name`.
- **A compiled schema with observers and no project name** does not boot. The compiler
  refuses to produce one; this covers a compiled schema written by hand.

A database whose observers were declared before owners carried a project name holds them
under the owner `schema`. The first boot of a named project takes over the ones it still
declares, in place (same id, same execution log, enabled or disabled as they were), and
soft-deletes the ones it does not, each with a `WARN`. Where two projects share such a
database, the first one to boot decides.

**Changing a declared observer.** Change the declaration and redeploy, or reload the
schema (below). The admin API
refuses to update or delete a declared observer's row with `409`, because the next boot would undo
it. The one exception is the switch: `POST /api/observers/{id}/disable` and `/enable`
(or a `PATCH` of `enabled` alone) work on a declared observer, so an operator can stop a
misbehaving webhook without a deploy. A disabled declared observer stays disabled across
deploys, and every boot logs a `WARN` naming it until it is enabled again.

**A schema reload applies the declared observers.** A reload (`SIGUSR1`, or the admin
reload endpoint) whose schema declares other observers writes them as boot does, with the
same table above, the same `WARN` lines and the same refusals, then rebuilds the running
observers from the stored rows. The next event is matched by the new declarations, with no
restart. Each replica does this when it reloads: the write is idempotent, and the advisory
lock makes a second replica find the rows already written rather than race to insert them.

- **All or nothing.** The rows are written, read back and built into the running observers
  in one transaction, committed after every other check of the reload has passed. A reload
  refused for any reason (a name another project holds, a variable a declared observer
  reads that is unset in production, or another section that needs a restart) leaves the
  rows and the running observers as they were. One case cannot be told apart: when the
  connection drops during the commit itself, the reload is refused while the rows may have
  been written. Reloading again, or the runtime's next `auto_reload`, makes them agree.
- **An event already matched** is delivered by the declaration it matched, retries
  included; only the events matched after the reload see the new one. The same holds for
  `POST /api/observers/runtime/reload`.
- **A dropped declaration** soft-deletes its row, as at boot, and a disabled declared
  observer stays disabled.
- **Mixed versions.** A replica that boots or reloads with an earlier schema writes the
  earlier declarations back, as at boot: the rows follow the last write. Replicas that have
  not reloaded yet pick up the rows on their next `auto_reload`.

`[observers]` itself (its runtime settings) and `[project] name` (the owner the rows are
stored under) are still read only at boot: a reload that changes them is refused.

### Where the tables live

The runtime's tables live in `core`, beside the change log: `core.tb_observer`,
`core.tb_observer_log`, `core.vw_observer_stats`, `core.observer_checkpoints` and
`core.tb_observer_dispatch`. Boot provisioning and every query name the schema, so the
connection's `search_path` does not decide which table is read (#1893).

Earlier releases created `tb_observer` and `observer_checkpoints` unqualified, in
whichever schema came first on that deployment's `search_path`, usually `public`. If
such a table still holds rows while its `core` counterpart is empty, the server refuses
to start and prints the statements that move it into `core`. Run them, then restart:

```sql
BEGIN;
DROP TABLE IF EXISTS core.tb_observer_log, core.tb_observer CASCADE;  -- empty, created by boot
ALTER TABLE public.tb_observer SET SCHEMA core;
ALTER TABLE IF EXISTS public.tb_observer_log SET SCHEMA core;
COMMIT;
BEGIN;
DROP TABLE IF EXISTS core.observer_checkpoints_history, core.observer_checkpoints CASCADE;
ALTER TABLE public.observer_checkpoints SET SCHEMA core;
ALTER TABLE IF EXISTS public.observer_checkpoints_history SET SCHEMA core;
COMMIT;
```

The next boot recreates the stats view, the helper functions and the triggers in `core`.
The old `public` view and functions are no longer used and can be dropped.

The compiled schema decides whether the runtime runs: `fraiseql.toml [observers] enabled`
compiles into its `observers_config`, and `fraiseql run` and `fraiseql-server` both read it
there. A server config's `[observers]` table tunes the runtime and does not start it. An
`enabled` written in the server config must agree with the compiled one, or the boot is
refused, naming `fraiseql.toml [observers]`. Diagnose a running runtime with the
server's own logs (`RUST_LOG=fraiseql_server=debug,fraiseql_observers=debug`).

---

## Environment Overrides

The server applies these over the compiled configuration at boot:

| Variable | Overrides |
|----------|-----------|
| `FRAISEQL_OBSERVER_TRANSPORT` | `[observers] backend` (`postgres`, `nats`, `in_memory`) |
| `FRAISEQL_NATS_URL` | `[observers] nats_url` |
| `FRAISEQL_NATS_SUBJECT_PREFIX` | `nats.subject_prefix` |
| `FRAISEQL_NATS_STREAM_NAME` | `nats.stream_name` |
| `FRAISEQL_NATS_CONSUMER_NAME` | `nats.consumer_name` |
| `FRAISEQL_NATS_ACK_WAIT_SECS` | `nats.jetstream.ack_wait_secs` |
| `FRAISEQL_NATS_MAX_MSGS` | `nats.jetstream.max_msgs` |
| `FRAISEQL_NATS_MAX_BYTES` | `nats.jetstream.max_bytes` |
| `FRAISEQL_NATS_DEDUP_WINDOW_MINUTES` | `nats.jetstream.dedup_window_minutes` |
| `FRAISEQL_NATS_MAX_AGE_DAYS` | `nats.jetstream.max_age_days` |
| `FRAISEQL_NATS_MAX_DELIVER` | `nats.jetstream.max_deliver` |
| `FRAISEQL_REDIS_URL` | `[observers.runtime.redis] url` (supplies the block when none is declared) |
| `FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS` | `[observers.runtime.redis] connect_timeout_secs` |
| `FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS` | `[observers.runtime.redis] command_timeout_secs` |

`[observers.runtime.redis]` is used only by `cache` observer actions. The server runs
no action-result cache. Nothing in the server reads `FRAISEQL_REDIS_POOL_SIZE`,
`FRAISEQL_REDIS_DEDUP_WINDOW_SECS` or `FRAISEQL_REDIS_CACHE_TTL_SECS`, so the server
refuses to boot when one of them is set.

The server always runs the observer executors. `[observers.runtime.transport]` takes only
the `nats` tuning: the stream, consumer and `jetstream` settings. Which transport runs and
the NATS URL are the compiled `[observers]` keys, so `transport` and `nats.url` written
there are refused at load, naming `fraiseql.toml [observers]`. The keys `run_bridge` and `run_executors` and a `[bridge]` table
are refused at load. The server refuses to boot when `FRAISEQL_NATS_RUN_EXECUTORS`,
`FRAISEQL_NATS_ENABLE_BRIDGE` or any `FRAISEQL_BRIDGE_*` variable is set.

The stream settings (`dedup_window_minutes`, `max_age_days`, `max_msgs`, `max_bytes`) apply
when the server creates the stream; a stream that already exists keeps its own. The
consumer settings (`ack_wait_secs`, `max_deliver`) are applied to the durable consumer at
every boot, so an existing consumer takes new values.

---

## Admin HTTP API

All routes are under `/api/observers`. They are mounted only when `[auth]` is
configured in `fraiseql.toml` (otherwise the server logs a `WARN` and skips them) and
the server has a PostgreSQL pool. Every request needs a valid bearer token carrying
the `fraiseql:admin` scope: no or invalid token returns 401, a token without the
scope returns 403.

### Tenant Scope

The token's tenant (the claim `[tenancy] tenant_claim` names, `tenant_id` by
default) decides what an admin acts on.

- **No tenant: the operator.** Every observer, every execution, and every route below.
  An observer the schema declares (`managed_by: "schema:<project name>"`) is the exception: the operator
  enables and disables it, and changes it in the schema.
- **A numeric tenant: that organisation** (`fk_customer_org`).
  - It creates observers it owns.
  - It lists and reads its own observers and the global ones (`fk_customer_org` null).
  - It changes, deletes, enables and disables only its own observers.
  - It sees logs and statistics only for its own observers. A global observer fires on
    every tenant's events, so its executions are the operator's to read.
  - Another organisation's observer answers 404.
- **Any other tenant: refused with 403.** `fk_customer_org` is a `BIGINT`, so a tenant
  such as `acme` or a UUID names no organisation the observer tables can scope by.

The changelog, the checkpoints, the runtime controls and the dead letter queue span every
tenant. They answer 403 to any token that names one.

A failed database query answers 500 with a generic message. The database's error text
goes to the server log only.

### Observer Management, Changelog and Checkpoints

| Method | Path | Does | Params |
|--------|------|------|--------|
| `GET` | `/api/observers` | List observers | `entity_type`, `event_type`, `enabled`, `include_deleted`, `page`, `page_size` |
| `POST` | `/api/observers` | Create an observer | JSON body |
| `GET` | `/api/observers/stats` | Statistics for all observers | — |
| `GET` | `/api/observers/logs` | Execution logs for all observers | `observer_id`, `status`, `event_id`, `trace_id`, `page`, `page_size` |
| `GET` / `PATCH` / `DELETE` | `/api/observers/{id}` | Read / update / soft-delete one observer | — |
| `POST` | `/api/observers/{id}/enable`, `/api/observers/{id}/disable` | Toggle one observer | — |
| `GET` | `/api/observers/{id}/stats`, `/api/observers/{id}/logs` | Per-observer statistics / logs | logs: as above |
| `GET` | `/api/observers/changelog` | Poll `core.tb_entity_change_log` | `after_cursor` (default 0), `limit` (default 100, max 1000), `object_type`, `latest` |
| `GET` | `/api/observers/checkpoint/{listener_id}` | Read a checkpoint (404 if none) | — |
| `PUT` | `/api/observers/checkpoint/{listener_id}` | Create or overwrite a checkpoint | body `{"last_cursor": <i64>}` |

An action sent to `POST /api/observers` or `PATCH /api/observers/{id}` is read by the
check the compiler applies to a declared observer. A key the runtime would ignore is
refused with `422`, naming the key, instead of being stored and doing nothing. A
`condition_expression` is checked against the entity type as a declared `condition` is:
a key no event can hold is refused with `422`, and a warning is logged, not returned, for
one only some events hold. An update checks the condition when it changes the condition
or the entity type; an update of the actions alone does not re-check a stored condition. An email
action is `{"type": "email", "to": …, "subject": …, "body_template": …}`: the recipient
and the subject are sent as written, and `cc`, `to_template`, `subject_template` and
`reply_to` are refused.

A key left out of `PATCH /api/observers/{id}` leaves that setting as it is.
`"condition_expression": null` clears the condition, so the observer fires on every
matching event; an empty condition is refused with `400`, as one that does not parse.
`"description": null` clears the description. `"retry_config": null` and
`"timeout_ms": null` reset the setting to what a create without it stores: three attempts
with exponential backoff from 1000 ms up to 60000 ms, and a 30000 ms timeout. On an
observer the schema declares, `null` is refused with `409`, as any change but `enabled` is.

A `retry_config` that leaves a field out takes that field from the same policy: 3 for
`max_attempts`, `exponential` for `backoff_strategy`, 1000 for `initial_delay_ms`, 60000
for `max_delay_ms`. The field means the same whether the server fills it in when it stores
the document or the runtime does when it reads a row written without it. A row whose
document the runtime cannot read runs on that policy too, and the server logs a warning
naming the observer.

### Runtime and Dead Letter Queue

Mounted only when the observer runtime is running (`[observers] enabled = true`).

| Method | Path | Does | Params |
|--------|------|------|--------|
| `GET` | `/api/observers/runtime/health` | `running`, `observer_count`, `last_checkpoint`, `events_processed`, `errors` | — |
| `POST` | `/api/observers/runtime/reload` | Reload observer definitions from the database | — |
| `GET` | `/api/observers/delivery/health` | Delivery summary, including DLQ, function-DLQ and dropped counts | — |
| `GET` | `/api/observers/dlq` | List DLQ items | `limit` (default 50), `offset` (default 0), `action` (action type, e.g. `webhook`), `object_type` (entity type) |
| `GET` | `/api/observers/dlq/stats` | `total_items`, `total_retries`, `dropped`, `by_action` | — |
| `GET` | `/api/observers/dlq/{id}` | One DLQ item | — |
| `DELETE` | `/api/observers/dlq/{id}` | Remove one DLQ item (404 if absent) | — |
| `POST` | `/api/observers/dlq/{id}/retry` | Re-dispatch one item; a failed retry re-inserts it | — |
| `POST` | `/api/observers/dlq/retry-all` | Re-dispatch every item | — |

The DLQ is held in the memory of each server process: it is per replica and does not
survive a restart. Cap it with `[observers.runtime] max_dlq_size`; at the cap the
newest failure is dropped and counted in `dropped`.

```bash
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  "https://api.example.com/api/observers/dlq?action=webhook&limit=20"
```

---

## Checkpoints

With the PostgreSQL transport the runtime keeps its cursor in `core.observer_checkpoints`
under the listener id `change_log`. It reads the cursor once at start and writes it
after every dispatched batch, so a running server overwrites any value set through
`PUT /api/observers/checkpoint/change_log`.

The cursor is only a scan bound. Whether a change-log row has been handled is decided
by the dispatch ledger `core.tb_observer_dispatch`, keyed by listener id: a row
recorded there is never dispatched again by that listener. Lowering the checkpoint
therefore does **not** replay already-dispatched rows. The checkpoint endpoints are
chiefly for external consumers that poll `GET /api/observers/changelog` and keep their
own `listener_id`.

Transports other than PostgreSQL persist no change-log checkpoint; delivery state is
owned by the transport (for NATS, the durable consumer).

---

## Scaling

**NATS.** Every replica with `backend = "nats"` subscribes through a durable
JetStream pull consumer named `nats.consumer_name` (default
`fraiseql_observer_worker`) with explicit acks. Replicas sharing the same
`consumer_name` and `stream_name` share that consumer and so act as competing
consumers: each message goes to one replica, and an unacknowledged message is
redelivered after `ack_wait_secs`, up to `max_deliver` deliveries. The server only
consumes the stream. Something else must publish change events to it.

**PostgreSQL.** Replicas share one listener id (`change_log`, not configurable in
`fraiseql.toml`), and with it one checkpoint and one dispatch ledger. Exactly one replica
polls `core.tb_entity_change_log`: the one holding a PostgreSQL advisory lock keyed on the
listener id. The others stand by and retry the lock every second. When the poller stops, or
PostgreSQL ends its session, a standby takes the lock at its next retry and resumes from the
stored checkpoint. Each replica keeps the lock on its own one-connection pool, outside the
request pool. The log says which replica polls ("Holding the change-log poll lease").

A row is recorded in the ledger after its actions run, so delivery stays at-least-once: a
poller that crashes, or loses its session, between running a batch's actions and recording
them leaves that batch to be dispatched again by the next poller. Make actions idempotent
where a repeat matters (see [observer-idempotency.md](observer-idempotency.md)).

**Subscriptions on every replica.** Dispatch and subscription delivery are separate reads.
Actions run once per deployment, as described above; GraphQL subscriptions and REST
`/{resource}/stream` deliveries are fed by each replica's own read, so a subscriber receives
every change whichever replica it is connected to:

- *PostgreSQL:* each replica tails `core.tb_entity_change_log` for its own subscribers,
  independently of the poll lease and the dispatch ledger, and writes nothing to the
  database. It polls every `poll_interval_ms` and hands each committed row out once, including
  a row whose transaction commits after a later row was already read. A replica delivers the
  changes committed after it started.
- *NATS:* each replica reads the stream through its own ephemeral JetStream consumer, next to
  the shared durable consumer that dispatch uses. It starts at new messages and acknowledges
  nothing; the broker removes it a minute after the replica stops pulling.

A change reaches subscribers whether or not its observer actions succeed.

The PostgreSQL read polls rather than using `LISTEN`/`NOTIFY` by measurement (#1158): a `pg_notify`
trigger on the change log serialises committing transactions (on PostgreSQL 18, 53 078 → 3 640
mutation transactions per second at 64 clients), while sixteen replicas polling at 10 Hz had
no measurable effect on write throughput.

---

## Metrics

With `observers-metrics`, the `fraiseql_observer_*` series are appended to the
server's `/metrics` scrape. `/metrics` itself is mounted only when `metrics_enabled`
is set together with `metrics_token`, and requires that token as a bearer token.

Series the server's dispatch path records:

| Metric | Labels |
|--------|--------|
| `fraiseql_observer_events_processed_total` | — |
| `fraiseql_observer_action_executed_total` | `action_type` |
| `fraiseql_observer_action_duration_seconds` | `action_type` |
| `fraiseql_observer_action_errors_total` | `action_type`, `error_type` |

The registry also defines backlog, dead letter queue, deserialization-failure and
action-result cache series. The server's dispatch path does not record all of them.
