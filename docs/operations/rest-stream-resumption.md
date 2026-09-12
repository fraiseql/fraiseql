# Resuming a REST event stream

`GET /rest/v1/{resource}/stream` is a Server-Sent Events feed of entity changes. A
browser `EventSource` reconnects on every network blip and re-sends the id of the last
event it received in a `Last-Event-ID` header. This page is what the server does with it.

## The short version

- Every event carries `id: <seq>`, the Change-Spine sequence of the change.
- Reconnecting with `Last-Event-ID: <seq>` **replays every event delivered after that one,
  in the order they were delivered**, and then continues live.
- Delivery is **at-least-once**: an event can repeat across a reconnect. The dedup key is
  `(object_type, seq)`, which is the key the whole Change Spine uses.
- A resume that cannot be honoured is **refused**, never answered with a partial replay.

## Why the resume point is not "every event with a higher seq"

This is the part that is easy to get wrong, and it looks correct while it loses data.

`seq` is allocated when a writing transaction inserts its change-log row, and the row
becomes visible when that transaction commits. Under concurrent writes the two orders
diverge:

```text
tx A: INSERT → seq 41, transaction still open
tx B: INSERT → seq 42, COMMITs first
poller (sees only B):  delivers seq 42 → your client stores Last-Event-ID: 42
tx A COMMITs
poller (next batch):   delivers seq 41 → arrives AFTER the higher seq
```

The id a client holds is the **last** event it received, never the **highest**. Reading
back "everything with `seq > 42`" would skip event 41 for ever, under a `200`, on a
stream that reports itself healthy — the same defect the change-log poller itself was
corrected for (#935), and the CDC enqueue cursor before it (#797).

So the replay does not order by `seq`. It reads the observer runtime's dispatch ledger
(`core.tb_observer_dispatch`), which records what was delivered and when, and reads
forward from your position in **that** order. It is a recorded fact rather than an
assumption about commit timing, and it returns exactly what you missed.

## What your deployment needs for it to work

**A PostgreSQL change-log poller.** Resumption is served from the dispatch ledger that
poller writes. With `[observers.runtime.transport] transport = "nats"` (or `in_memory`)
the runtime's events never pass through that ledger — the local change log may not even
be their source — so there is nothing to resume from, and every `Last-Event-ID` request
is answered `501 RESUMPTION_UNSUPPORTED`. That is the truth about such a deployment, not
a missing feature.

**Migrations 08 and 14 applied at their current version.** They add the two indexes the
resume query needs — `idx_entity_log_id` on the change log and
`idx_observer_dispatch_dispatched` on the ledger. Without them the query still returns
the right answer, but it hash-joins the whole ledger against every row of the entity
type: 13 ms on a 60 000-row log, and linear in the log from there. The ledger index is
installed automatically on the poller's first run; the change-log one comes with
migration 08.

**Retention that prunes the ledger no more aggressively than the change log**, which is
what migration 14 already asks for. A resume point whose change-log row has been pruned
cannot be honoured — see `410` below.

## Refusals

Each one is a distinct code, because "cannot resume" alone leaves you guessing between a
pruned log, a deployment that records nothing, and a client too far behind. Note that a
browser `EventSource` stops reconnecting on any non-`200`, which is intended: a client
that cannot resume should find out.

| Status | Code | Means |
|---|---|---|
| `400` | `RESUME_POINT_INVALID` | The header is not an id this stream issues. Ids are integers (`seq`). |
| `410` | `RESUME_POINT_UNKNOWN` | No event on this stream carries that sequence — it aged out of the change log, or the id came from another resource, tenant or deployment. What followed it cannot be established, so it is refused rather than answered from the top of the log. |
| `413` | `RESUME_TOO_FAR_BEHIND` | More events have been delivered since that point than `[rest].sse_max_replay_events` allows replaying. Refused before the first frame: a truncated replay is indistinguishable from a complete one. |
| `501` | `RESUMPTION_UNSUPPORTED` | This deployment keeps no record of what the stream delivered (see above), or there is no observer runtime at all. |

The client's way out of all four is the same and is stated in every refusal body:
reconnect **without** the header and knowingly receive events from now on.

## Configuration

```toml
[rest]
# How far back a resume may reach, counted in delivered events across all
# entity types — which is what the walk through the ledger costs.
# 0 disables the bound.
sse_max_replay_events = 10000
```

`0` is the permissive setting and worth naming as such: on a stream reachable without a
credential it hands an anonymous client an unbounded read of your busiest table.

## Duplicates, and where they come from

Two places, both deliberate:

- **The in-flight tail.** The poller publishes a batch to subscribers and records it in the
  ledger afterwards — that order is what makes delivery at-least-once. A resume therefore
  also reads the rows the ledger has not placed yet, which includes rows the poller simply
  has not reached; those arrive again on the live stream a moment later. Within one
  connection they are suppressed; across a reconnect they are not.
- **A crash between dispatch and record.** The batch is re-delivered on restart, unchanged
  from before resumption existed.

Dedup on `(object_type, seq)`.

## What ends a stream

- **`event: error` with `code: STREAM_LAGGED`** — the client fell a whole buffer behind and
  events were dropped before they could be delivered. The stream ends rather than
  resuming past a gap the client cannot see. Reconnecting with `Last-Event-ID` now
  closes that gap, which is what makes this the supported recovery path rather than a
  dead end.
- **`event: error` with `code: REPLAY_FAILED`** — the catch-up read failed part way. The
  stream ends rather than continuing live with a hole in it. Reconnect to try again.

## An event with no sequence

A change-log row whose producer wrote no `seq` is delivered with **no `id:` field at
all**. Per the SSE specification that leaves the client's last-event-id buffer unchanged,
so a reconnect still names the last event that had one — at-least-once, never a skip.
Such an event is replayed like any other; it simply cannot itself be a resume point,
exactly as it cannot be one on the live stream.

## See also

- [graphql-sse-streaming.md](graphql-sse-streaming.md) — the *other* SSE surface (`@stream`
  / `@defer` over a GraphQL document), which resumes by re-executing from an offset and
  is unrelated to this one.
- `docs/architecture/change-log-contract.md` — the Change Spine, `seq`, and the
  change-log contract this replay reads.
