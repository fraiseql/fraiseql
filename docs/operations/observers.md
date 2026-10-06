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
(`enabled`, `backend`, `handlers`); the server's own tuning lives under
`[observers.runtime]` (`poll_interval_ms`, `batch_size`, `channel_capacity`,
`auto_reload`, `reload_interval_secs`, `max_dlq_size`, `log_payloads`) with the
sub-tables `[observers.runtime.transport]`, `[observers.runtime.email]`,
`[observers.runtime.pool]` and `[observers.runtime.redis]`. `[observers.runtime]`
rejects unknown keys, so a typo stops the server at boot instead of being ignored.

`[observers] enabled = false` starts no runtime. Diagnose a running runtime with the
server's own logs (`RUST_LOG=fraiseql_server=debug,fraiseql_observers=debug`).

---

## Environment Overrides

The server applies these over the compiled configuration at boot:

| Variable | Overrides |
|----------|-----------|
| `FRAISEQL_OBSERVER_TRANSPORT` | `transport` (`postgres`, `nats`, `in_memory`) |
| `FRAISEQL_NATS_URL` | `nats.url` |
| `FRAISEQL_NATS_SUBJECT_PREFIX` | `nats.subject_prefix` |
| `FRAISEQL_NATS_STREAM_NAME` | `nats.stream_name` |
| `FRAISEQL_NATS_CONSUMER_NAME` | `nats.consumer_name` |
| `FRAISEQL_NATS_ACK_WAIT_SECS` | `nats.jetstream.ack_wait_secs` |
| `FRAISEQL_NATS_MAX_MSGS` | `nats.jetstream.max_msgs` |
| `FRAISEQL_NATS_MAX_BYTES` | `nats.jetstream.max_bytes` |
| `FRAISEQL_REDIS_URL` | `[observers.runtime.redis] url` (supplies the block when none is declared) |
| `FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS` | `[observers.runtime.redis] connect_timeout_secs` |
| `FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS` | `[observers.runtime.redis] command_timeout_secs` |

`[observers.runtime.redis]` is used only by `cache` observer actions. The server runs
no Redis deduplication or action-result cache, and refuses to boot when
`FRAISEQL_REDIS_POOL_SIZE`, `FRAISEQL_REDIS_DEDUP_WINDOW_SECS` or
`FRAISEQL_REDIS_CACHE_TTL_SECS` is set. `FRAISEQL_ENABLE_DEDUP`,
`FRAISEQL_ENABLE_CACHING`, `FRAISEQL_JOB_QUEUE_*` and `FRAISEQL_CLICKHOUSE_*`
configure the `fraiseql-observers` library only. The remaining NATS JetStream and
bridge variables are read but not applied by the server (issue #1496).

---

## Admin HTTP API

All routes are under `/api/observers`. They are mounted only when `[auth]` is
configured in `fraiseql.toml` (otherwise the server logs a `WARN` and skips them) and
the server has a PostgreSQL pool. Every request needs a valid bearer token carrying
the `fraiseql:admin` scope: no or invalid token returns 401, a token without the
scope returns 403.

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

With the PostgreSQL transport the runtime keeps its cursor in `observer_checkpoints`
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

**NATS.** Every replica with `transport = "nats"` subscribes through a durable
JetStream pull consumer named `nats.consumer_name` (default
`fraiseql_observer_worker`) with explicit acks. Replicas sharing the same
`consumer_name` and `stream_name` share that consumer and so act as competing
consumers: each message goes to one replica, and an unacknowledged message is
redelivered after `ack_wait_secs`. The server only consumes the stream: it does not run
the PostgreSQL → NATS bridge (`run_bridge` is not applied, issue #1496).

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

GraphQL subscriptions and REST `/{resource}/stream` deliveries are fed by the events the
replica's own runtime consumed, so a subscriber connected to a standby replica receives no
change events (on NATS, each replica's subscribers see the share of messages that replica
consumed). Route subscribers to the polling replica; issue #1503 tracks per-replica fan-out.

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

The registry defines further `fraiseql_observer_*` series (cache, deduplication, job
queue) that only the corresponding library components record.
