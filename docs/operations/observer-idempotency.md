# Observer Delivery and Idempotent Consumers

Observer actions are delivered **at least once**. `fraiseql-server` does not suppress
duplicates: an action can run more than once for the same change, and a consumer whose
side effect must not repeat (a charge, an email, a ledger entry) has to recognise the
repeat itself. This page describes when repeats happen, the key that identifies them, and
how to build a consumer that ignores them.

For running observers in general (admin API, DLQ, checkpoints, scaling), see
[observers.md](observers.md).

---

## Why Delivery Is At-Least-Once

An action is an external effect: an HTTP request, an email, a Slack message. It cannot
commit in the same transaction as the record that says it ran, so one of the two always
happens first. FraiseQL runs the action first and records it afterwards. A failure between
the two re-runs the action; the other order would lose it.

With the PostgreSQL transport the record is the **dispatch ledger**,
`core.tb_observer_dispatch`, keyed by listener id (`change_log` under `fraiseql-server`). The
poller runs a batch's actions, then records every row of the batch in the ledger, then
advances its checkpoint. A row recorded in the ledger is never dispatched again by that
listener, whatever the checkpoint says.

An action runs again for the same change-log row when:

| Situation | What repeats |
|-----------|--------------|
| The server crashes, or loses its database session, after a batch's actions ran and before the ledger write | Every action of that batch, on the next poller |
| The ledger write fails (logged as `Failed to record dispatched change-log rows`) | That batch, on the next poll |
| An action fails or times out after the receiver already acted (e.g. the response was lost) | That action, on retry (per the observer's `retry` settings) |
| An operator retries a DLQ item (`POST /api/observers/dlq/{id}/retry` or `retry-all`) | That action |
| An operator clears the ledger to replay (see [Replaying from the Start](#replaying-from-the-start)) | Every action of every row still in the change log |

With the NATS transport the record is the JetStream acknowledgement. A message that is not
acknowledged within `ack_wait_secs` is redelivered, up to `max_deliver` deliveries, so the
same repeats apply.

---

## The Dedup Key: the Event Id

Every event has an id, and a repeat carries the same id as the original:

- **PostgreSQL transport:** the id is the change-log row's `id` column
  (`core.tb_entity_change_log.id`, a UUID). It is fixed when the row is written, so every
  dispatch of that row, including a replay after a crash or a DLQ retry, carries it.
- **NATS transport:** the id is the `id` of the event the publisher put on the stream. The
  server consumes the stream as published, so a publisher that reuses the change-log row
  UUID gives the same guarantee. JetStream also drops a re-published message whose
  `Nats-Msg-Id` it has seen within `dedup_window_minutes`.

Webhooks receive it in the **`X-FraiseQL-Event-Id`** header. The request body is the entity
row (or the observer's body template over it), which does not identify the event. An
operator header of the same name in the observer's configuration is not sent, so the key
cannot be replaced by a constant.

The header is not covered by `X-FraiseQL-Signature-256`, which signs the timestamp and the
body only. Verify the signature first, and rely on its timestamp tolerance to bound how long
a captured request can be replayed.

Other action types (`email`, `slack`, `sms`, `push`) deliver to a person or a provider and
do not carry the id. `search` and `cache` actions are idempotent by construction: they
re-index or invalidate the same entity.

---

## Building an Idempotent Consumer

Record the event id in the same transaction as the side effect, and skip the effect when the
id is already recorded. A unique constraint makes the check race-free across concurrent
deliveries:

```sql
CREATE TABLE processed_event (
    event_id     UUID        PRIMARY KEY,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

```python
def handle_order_webhook(request):
    verify_signature(request)                      # X-FraiseQL-Signature-256
    event_id = request.headers["X-FraiseQL-Event-Id"]

    with db.transaction():
        inserted = db.execute(
            "INSERT INTO processed_event (event_id) VALUES (%s) "
            "ON CONFLICT (event_id) DO NOTHING",
            (event_id,),
        ).rowcount
        if inserted == 0:
            return 200                             # a repeat: already handled
        charge_customer(request.json())            # runs once per event id
    return 200
```

Answer a repeat with a 2xx status. A 4xx answer sends the action straight to the DLQ, and a
5xx or 429 answer is retried.

When the side effect calls a third-party API that takes an idempotency key (most payment
providers do), pass the event id as that key as well. The provider then rejects the repeat
even when your own transaction fails after its call.

Keep processed ids for at least as long as a repeat can arrive: the longest of your retry
window, your DLQ retention, and how far back an operator might replay. Delete older rows
with a scheduled job:

```sql
DELETE FROM processed_event WHERE processed_at < now() - INTERVAL '30 days';
```

---

## Observing Repeats

`fraiseql-server` does not detect duplicates, so no server metric counts them. Count them on
the consumer side, from the conflicts in your own table. On the server, the signals that an
action may run again are:

- the log line `Failed to record dispatched change-log rows; this batch may be re-delivered`;
- `fraiseql_observer_action_errors_total` (with `observers-metrics`), for actions that will
  be retried;
- the DLQ (`GET /api/observers/dlq/stats`), for actions an operator may retry.

---

## Replaying from the Start

Lowering the checkpoint does not replay anything: the ledger still holds every dispatched
row. To dispatch every row in `core.tb_entity_change_log` again, stop the server first (a
running runtime overwrites the checkpoint after every batch), then clear both:

```sql
DELETE FROM core.tb_observer_dispatch WHERE listener_id = 'change_log';
DELETE FROM observer_checkpoints WHERE listener_id = 'change_log';
```

On restart every action runs again for every row, with the same event ids, so an idempotent
consumer skips the rows it already handled.
