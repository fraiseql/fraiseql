# GraphQL over SSE and `@stream`

FraiseQL can deliver GraphQL responses as Server-Sent Events, including
incremental row delivery for large list queries (#387). The transport is
**opt-in** and default-off.

## Configuration

```toml
enable_graphql_sse = true

# Rows per continuation batch after the initial payload. Optional; default 100.
# Setting it without enable_graphql_sse is a configuration error.
graphql_sse_stream_batch_size = 100
```

## Negotiation

A request to the GraphQL endpoint (GET or POST) carrying
`Accept: text/event-stream` receives its response as SSE. With the feature
disabled, the header is ignored and behaviour is byte-for-byte unchanged.

The SSE branch lives **inside** the `/graphql` route, so it sits behind the
same authentication layer, content-type enforcement, body limits, rate
limiting and timeout middleware as the buffered transport. An unauthenticated
request is refused with the ordinary HTTP 401 before any stream opens.

## Single-result mode

Any operation without `@stream` (including mutations over POST) executes
through the ordinary buffered pipeline once and is delivered as one `next`
event carrying the standard execution result, followed by `complete`:

```
event: next
data: {"data":{"items":[…]}}

event: complete
data:
```

## `@stream` on a root list field

```graphql
{ items(orderBy: {id: ASC}) @stream(initialCount: 2) { id label } }
```

- The initial `next` payload carries `initialCount` rows plus `hasNext: true`.
- Continuation batches follow as incremental payloads:
  `{"incremental":[{"items":[…],"path":["items",N]}],"hasNext":true}`.
- The final payload carries `hasNext: false`; the stream ends with `complete`.
- `@stream(if: false)` falls back to single-result mode.

**Every batch re-executes the full pipeline** — depth/complexity gates,
operation authorization, RLS session variables, the result cache — by
re-issuing the same document with paginated `limit`/`offset` variables.
There is no separate streaming execution path to drift. Operation cost is
charged once per logical request; the audit trail records the delivery on the
`fraiseql::sse_audit` target with batch and row counts.

### Eligibility

`@stream` is accepted only on the single root field of a query operation, and
only when the compiled query returns a list and accepts `limit`/`offset`
parameters. Everything else is refused loudly before the stream opens:

- non-list (single-item) queries,
- relay (connection) queries — use cursor pagination instead,
- nested `@stream`, multi-root operations, mutations,
- documents declaring `$limit` or `$offset` variables (batching injects those
  variables; silently overriding the client's would corrupt nested arguments).

Outside a negotiated SSE request, `@stream` and `@defer` are known, advisory
no-ops (the incremental-delivery proposal permits serving the full result) —
they parse, evaluate as include, and produce no warnings.

## Consistency and lifecycle

- Batches are separate statements, not one snapshot: concurrent writes can
  shift rows between batches. Use a deterministic `orderBy`, and prefer the
  primary-pinned window after writes when combined with read replicas.
- Long-lived deliveries re-check the principal before every batch (the same
  rule subscriptions follow): an expired token terminates the stream with an
  `UNAUTHENTICATED` error event followed by `complete`. The executor
  additionally refuses expired contexts per batch as defence in depth.
- A client disconnect drops the response stream, which stops the batch loop;
  no further database statements are issued for that delivery.
- The delivery survives `request_timeout_secs` (the timeout bounds
  response-head production, not the streaming body), and SSE responses are
  exempt from response compression (a buffering encoder would defeat
  incremental flushing).

## Not (yet) supported

`@defer` on fragments, `multipart/mixed` incremental delivery, nested
`@stream`, database-cursor row streaming (the fraiseql-wire integration),
resumable streams via `Last-Event-ID`, and mid-stream revocation re-checks are
tracked in the follow-up issue.
