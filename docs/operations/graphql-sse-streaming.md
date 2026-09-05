# Incremental GraphQL delivery: `@stream`, `@defer`, SSE and `multipart/mixed`

FraiseQL can deliver a GraphQL response incrementally — row batches for a large
list (`@stream`, #387) and a split payload for a deferred fragment
(`@defer`, #958) — over either of two wire framings. The capability is
**opt-in** and default-off.

## Configuration

```toml
enable_graphql_incremental = true

# Rows per continuation batch after the initial payload. Optional; default 100.
# Setting it without enable_graphql_incremental is a configuration error.
graphql_incremental_batch_size = 100
```

## Negotiation

A request to the GraphQL endpoint (GET or POST) is delivered incrementally when
its `Accept` header names one of two framings:

| `Accept` | Framing |
|---|---|
| `text/event-stream` | GraphQL-over-SSE: one `next` event per payload, then `complete` |
| `multipart/mixed` | the Apollo/Relay framing (`deferSpec=20220824`): one MIME part per payload, then the closing boundary |

Both carry the **same payload sequence** — they are two envelopes over one
delivery, built from one code path so they cannot drift. A request naming both
gets SSE. With the feature disabled, both headers are ignored and behaviour is
byte-for-byte unchanged.

`multipart/mixed` has no terminal event: the last payload's `hasNext: false` is
the end-of-delivery signal, which is why every payload carries `hasNext`.

The incremental branch lives **inside** the `/graphql` route, so it sits behind
the same authentication layer, content-type enforcement, body limits, rate
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

Outside a negotiated incremental request, `@stream` and `@defer` are known,
advisory no-ops (the incremental-delivery proposal permits serving the full
result) — they parse, evaluate as include, and produce no warnings.

## `@defer` on a fragment

```graphql
{ items(limit: 2) { id ...Detail @defer(label: "detail") } }
fragment Detail on SseItem { label }
```

The immediate payload carries the fields the client did not defer, with
`hasNext: true`; each deferred fragment then arrives as an `incremental` entry
addressed by its response path, one per list element:

```
{"data":{"items":[{"id":1},{"id":2}]},"hasNext":true}
{"incremental":[{"data":{"label":"one"},"path":["items",0],"label":"detail"}],"hasNext":true}
{"incremental":[{"data":{"label":"two"},"path":["items",1],"label":"detail"}],"hasNext":false}
```

**`@defer` here splits the delivery, not the query plan.** A FraiseQL query is
one SQL statement over a JSONB view — there are no per-field resolvers to defer —
so the deferred fields are produced by the same statement as the immediate ones
and then held back. Two consequences, stated plainly so neither is inferred
wrongly:

- It does **not** reduce database work. It changes when bytes reach the client, which is
  a real benefit when the deferred part is large (a big nested list) and close to nothing
  when it is small.
- It is **always correctly aligned**, including inside lists. The alternative —
  re-querying the deferred fields in a second statement — would genuinely save work in
  the first, and would attach one row's deferred fields to another row whenever a
  concurrent write shifted the window between the two snapshots. That trade was refused.

`@defer(if: false)` leaves the response in its ordinary single-payload shape.
`@defer` combined with `@stream` in one operation is refused: the two order the
same response differently and interleaving them is not defined here.

## Consistency and lifecycle

- Batches are separate statements, not one snapshot: concurrent writes can
  shift rows between batches. Use a deterministic `orderBy`, and prefer the
  primary-pinned window after writes when combined with read replicas.
  This is a property of `@stream` specifically, and it is the price of a delivery
  that can be **resumed**: each batch re-executes the query, which is what makes
  `Last-Event-ID` work without a replay buffer. The REST exports made the opposite
  trade — one statement, one snapshot, no resume — see
  [the export note](#a-note-on-the-rest-exports) below.
- Long-lived deliveries re-check the principal before every batch (the same
  rule subscriptions follow, through the same guard): an expired **or revoked**
  token terminates the stream with an `UNAUTHENTICATED` error event followed by
  `complete`. The executor additionally refuses expired contexts per batch as
  defence in depth. Revocation covers both a single-token `jti` revoke and a
  per-user `revoke-all` epoch, and applies to any caller the auth layer
  authenticated with a JWT.
- A client disconnect drops the response stream, which stops the batch loop;
  no further database statements are issued for that delivery.
- The delivery survives `request_timeout_secs` (the timeout bounds
  response-head production, not the streaming body), and incremental responses on
  both framings are exempt from response compression (a buffering encoder would
  defeat incremental flushing).

## Nested `@stream`

`@stream` on a list **inside** a row — `users { posts @stream(initialCount: 2) }` —
is supported, and is a different mechanism from the root-field form above. It splits
the *delivery* of a list the query already produced, exactly as `@defer` splits the
delivery of a deferred fragment:

```
event: next
data: {"data":{"users":[{"id":1,"posts":["p1","p2"]}]},"hasNext":true}

event: next
data: {"incremental":[{"items":["p3","p4"],"path":["users",0,"posts",2]}],"hasNext":false}
```

The `path` addresses the position of the chunk's **first item**, so a client splices
it back at that index. Each element of an enclosing list gets its own path
(`["users",0,…]`, `["users",1,…]`).

Why it is a delivery split and not paging: a nested list is a JSONB array produced by
the same single statement as the row that carries it. There is no per-path pagination
to push down — the array is already in memory by the time anything could page it, and
fetching "the next 10 posts of user 3" would be a second statement over a second
snapshot, with the alignment problem described under `@defer` above. The honest
consequences:

- it does **not** reduce database work, and does not bound server memory;
- it is always correctly aligned, because there is one snapshot;
- it is **not** resumable. A root `@stream`'s event id is a row offset the query
  accepts as an argument; a nested chunk boundary is a position in a value that no
  longer exists once the response is delivered. Nested-`@stream` payloads carry no
  `id:`.

`initialCount` (default 0) is how many items stay in the immediate payload;
`graphql_incremental_batch_size` sizes the chunks. `@stream(if: false)` leaves the
list alone. A `@stream` on a field that resolved to something other than a list is
**refused** with an ordinary HTTP error — the split happens before any byte of the
response is written, and a directive that silently did nothing on a negotiated
incremental transport would read as "streaming worked".

Two combinations are refused rather than resolved one way: a nested `@stream` with a
`@defer` (both split one result and their payload order is undefined here), and a
nested `@stream` with a root `@stream` (each root batch would carry its own copy of
the nested list, which has no incremental addressing).

### A note on the REST exports

The REST export representations (`Accept: application/x-ndjson`, `text/csv`, XLSX) and
gRPC server-streaming deliver rows from **one statement over one database portal**, so
they are a single snapshot and cost `O(N)` rather than `O(N²)` in row scans. They are
not resumable, and they hold a pooled connection for the life of the export — bounded
by `pool_max_streaming_reads` (see the connection-pool runbook).

The REST exports are a **per-route opt-in**: a query offers them only with
`rest_stream = true`, and a route without it answers `406 Not Acceptable` to those
`Accept` values while serving its JSON envelope unchanged. An export is not a bigger
page — it reads the whole filtered relation, is not bounded by `max_page_size`, and
holds that connection for as long as the client reads — so which routes offer one is
the schema author's decision.

```python
@fraiseql.query(sql_source="v_invoice", rest_stream=True)
def invoices() -> list[Invoice]: ...
```

### What an export refuses

An export answers `400 Bad Request` to anything that asks it to be a page rather than a
whole result set: `Prefer: count=…`, `?offset=`, any of `?first=`/`?after=`/`?last=`/
`?before=`, and a `?select=` naming an embedded relationship or an embedded count. Ordinary
field filters, sorts and a plain `?select=` are honoured as usual.

It also refuses `?rel.field=value`, the filter syntax for an embedded relationship, because
an export carries no embed for it to narrow. That refusal does not depend on the name being
a relationship the type declares — nothing checks, so `?nonsense.field=x` is refused the same
way. Filter the exported rows themselves with `?field=value`, or request
`Accept: application/json` to embed and filter.

`?limit=` is the exception. On an export it bounds the **total** rather than a page, it is
not clamped to `max_page_size`, and leaving it out means the whole table.

The refusal is about what the *request* asked for, not about how the route paginates: a
`relay = true` query with `rest_stream = true` exports normally as long as the request names
no cursor. Relay routes reject `?limit=` (they take `?first=`, not `?limit=`), so an export
of one is currently always the whole relation.

The two are deliberately different, because the questions differ. An export is one
transfer of a result set to a file; a `@stream` delivery is an interactive rendering
that a browser may reconnect to. A snapshot cannot survive a reconnect, and a resume
point cannot exist without re-execution.

## Resuming a dropped delivery

Every `next` event carries an `id:` — the **absolute row offset of the first row
that event did not deliver**. Reconnect with the same document and
`Last-Event-ID: <that id>` and the delivery continues from exactly there: no row
repeated, none skipped. A browser `EventSource` sends the header automatically;
other clients set it themselves.

```
id: 2
event: next
data: {"data":{"items":[{"id":1},{"id":2}]},"hasNext":true}
```

```http
POST /graphql
Accept: text/event-stream
Last-Event-ID: 2
```

No replay buffer is involved, and none would be honest: the source is a
re-executable paginated query, not a transient event feed. Two consequences worth
knowing:

- Rows already delivered are **charged against the document's own `limit`**. Resuming a
  `limit: 100` delivery that got 40 rows through delivers at most 60 more, not 100.
- Because batches are separate statements, a row inserted before the resume offset
  between the two connections shifts the window, exactly as it does between batches of
  one connection. A deterministic `orderBy` bounds this; a snapshot does not exist across
  a reconnect by definition.
- A `Last-Event-ID` that is not an offset, or that points before the document's own
  `offset` argument, is refused with a `400`-shaped GraphQL error rather than clamped —
  a silently adjusted resume point returns a wrong result set that looks like a right
  one.

The terminal payload of a delivery that ended early (revoked token, batch error)
carries an id too, so a client that fixes the cause resumes rather than restarts.

## Not (yet) supported

`Last-Event-ID` on the REST observer stream (`/rest/v1/{resource}/stream`) is #1113, and
separate: that transport's event id is an event UUID over a live feed, not an offset
into a re-executable query.
