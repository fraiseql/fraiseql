# Cache Module

**Source files**:

- `crates/fraiseql-core/src/cache/adapter/` — `CachedDatabaseAdapter`, the main entry point (split into `mod.rs`, `query.rs`, `mutation.rs`, `tests.rs`)
- `crates/fraiseql-core/src/cache/result.rs` — `QueryResultCache` (`moka::sync::Cache`, W-TinyLFU eviction, lock-free reads) and `CachedResult`
- `crates/fraiseql-core/src/cache/key.rs` (~632 lines) — cache key construction and security model
- `crates/fraiseql-core/src/cache/cascade_invalidator.rs` — view dependency graph and cascade invalidation
- Supporting: `invalidation_api.rs`, `fact_table_cache.rs`, `relay_cache.rs`, `config.rs`

**Tests**: Unit tests in `#[cfg(test)] mod tests` inside each source file. Run with:

```bash
cargo nextest run -p fraiseql-core --lib cache
```

---

## Overview

The cache module wraps `DatabaseAdapter` in a `CachedDatabaseAdapter` that stores query
results in an in-process `moka::sync::Cache` (W-TinyLFU).

**It is opt-in per query.** `cache_enabled = true` (the server default) turns the cache
on, but a read is cached only when its query declares `cache_ttl_seconds`, authored in
the SDK or lowered from `[[caching.rules]]`; every other read bypasses it. A schema that
declares no TTL caches nothing, and the server warns at boot when that is so. There is
deliberately no server-wide default TTL: it would serve bounded-stale reads to every
query, including ones whose author never chose that trade (#1555). Reads that carry
session variables are cached too, keyed by those variables (#1373).

The design has three critical properties:

1. **RLS isolation** — the cache key includes the user's Row-Level Security WHERE clause,
   ensuring that two users with different RLS policies never share cached data.
2. **View-based invalidation** — invalidation is coarse-grained (per-view, not per-row),
   which is fast but causes over-invalidation on write-heavy workloads. View-to-key
   reverse indexes are `DashMap`-based and updated on `put()` and via moka's eviction
   listener.
3. **Lock-free reads, W-TinyLFU eviction** — `moka::sync::Cache` performs frequency
   tracking via thread-local ring buffers; cache hits acquire no shared lock. Metrics
   counters are `AtomicU64`/`AtomicUsize`. This replaces an earlier 64-shard
   `parking_lot::Mutex<LruCache>` design that was the hot-key serialisation bottleneck.

---

## Cache Key Construction

The cache key is `SHA-256(hash_query_with_variables + serde_json_where + schema_version)`.

### Why each component matters

| Component | Reason |
|-----------|--------|
| `hash_query_with_variables(query, vars)` | Different variables must produce different keys; APQ hasher normalizes variable ordering |
| `serde_json::to_string(WHERE clause)` | RLS produces per-user WHERE clauses; different users must hit different cache entries |
| Schema version hash | Invalidates all entries on deployment when schema changes |

### RLS security model (critical)

Without the RLS WHERE clause in the key, user A's cached result could be served to user B.
This would be a data breach. The cache key is the security boundary.

The APQ module normalizes variables before hashing, so:

- `{a: 1, b: 2}` and `{b: 2, a: 1}` produce the **same** key ✅
- `{a: 1}` and `{a: 2}` produce **different** keys ✅
- User A with `WHERE tenant_id = 1` and User B with `WHERE tenant_id = 2` produce **different** keys ✅

`serde_json::to_string` is used (not `{:?}` Debug format) to ensure key stability when
internal Rust types are refactored.

---

## Per-Entry TTL

```
QueryDefinition.cache_ttl_seconds: Option<u64>   (from compiled schema)
      ↓ (loaded at server startup)
CachedDatabaseAdapter.view_ttl_overrides: HashMap<ViewName, u64>
      ↓ (applied per put() call)
CachedResult { data, views_depended_on, cached_at, ttl_seconds }
```

**TTL = 0 means: no time-based expiry.** The entry is stored and lives until a
mutation evicts it — this is how mutation-invalidated-only queries are declared.
It does **not** mean "never cache": a query stays out of the cache by having no
`cache_ttl_seconds` at all, as the injected change-log queries do (#1590). Only a
mutation run by the same server evicts it: with several servers, see
[Several servers](#several-servers-1577).

**TTL expiry is delegated to moka** via `expire_after_create`; there is no
manual read-time expiry check (a deliberate perf decision — see
`CacheEntryExpiry` in `cache/result.rs`).

### Declarative rules (`[[caching.rules]]`, #623)

`fraiseql.toml` can author per-query caching without the SDK:

```toml
[caching]
enabled = true

[[caching.rules]]
query = "inventory"            # must name an existing query
ttl_seconds = 120              # lowered onto QueryDefinition.cache_ttl_seconds
invalidation_triggers = ["createOrder"]  # mutations that must evict this query
```

Rules are lowered **at compile time** onto the two compiled fields above:
the TTL onto the query, and the query's view into each trigger mutation's
`invalidates_views`. Unknown query/mutation names, a TTL already authored via
the SDK, `enabled = false` with rules, and a `backend`/`redis_url` (no Redis
result cache exists) are all compile errors. Runtime enablement remains the
server's `cache_enabled`; the server warns at boot when TTLs are declared but
the cache is off, and when the cache is on but no query declares a TTL.

### HTTP caching of `/graphql` responses (#1564)

The same `cache_ttl_seconds` sets how long a browser or a CDN may reuse a `GET /graphql`
response (`Cache-Control: max-age`). A client can name a persisted query by its hash in the
URL, so the URL is a stable cache key:

```text
GET /graphql?extensions={"persistedQuery":{"version":1,"sha256Hash":"<sha256 of the query>"}}
```

The hash must already be registered, by a `POST` carrying the query and its hash (the
standard APQ handshake). A `GET` with the hash and the query runs it but stores nothing:
registering is a write, and `GET` does not write. Nor does `GET` (or `QUERY`) run a
mutation, whether it arrives as text or by hash: the operation is judged on the resolved
document.

| The response | `Cache-Control` |
|---|---|
| to a `POST` or `QUERY`, or an SSE stream | `no-store` |
| carrying `errors`, even with `data` | `no-store` |
| to an operation one of whose root fields has no `cache_ttl_seconds`, or `0` | `no-store` |
| to a caller with a principal, an `Authorization` header, a cookie or an `X-API-Key`; to a tenant (by `X-Tenant-ID` or `Host`); under a schema reading a session variable from a header | `private, max-age=<ttl>` |
| otherwise | `public, max-age=<ttl>` |

`<ttl>` is the smallest TTL among the operation's root fields. `Vary` lists `Authorization`,
`Cookie`, `X-API-Key`, `X-Tenant-ID`, and every header the schema reads a locale or a
session variable from.

Two differences from the server's result cache:

- **A mutation does not reach an HTTP cache.** The server evicts its own entries when a
  mutation touches their views; a browser or a CDN keeps a response for its full `max-age`.
  Declare a TTL the data can be that stale for.
- **A TTL of `0` caches nothing over HTTP.** In the result cache it means "no time-based
  expiry, evicted by mutations"; an HTTP cache has no such eviction, so the response is
  `no-store`.

---

## What a write evicts

A successful mutation evicts every cached entry of every view it reaches. It reaches:

1. The views it names: its `invalidates_views`, its return type's view (and the view of the
   entity a payload type wraps), the type its function stamped as `entity_type`.
2. Its cascade: the view of every type in `updated` and `deleted`, and the views of every
   query an `invalidations` hint names (the query's `sql_source` and `additional_views`). A
   cascade derived from `pg_tviews` (`cascade_source = "pg_tviews"`) also reaches the view of
   every type the report says changed, including the types it truncated or could not list.
3. **The views that embed those** (#1575), read from the database at boot:

```text
updateUser sweeps tv_user
      ↓ the embedding graph (read from the catalog at boot)
tv_post embeds tb_user (pg_tviews: cascade_kinds tb_user = mapped)
v_post_full reads tv_post (pg_depend)
      → evicts tv_user, tv_post, v_post_full
```

A view **reads** itself, every relation it reaches through view definitions (transitively,
including views the schema never names), and the base tables of every `pg_tviews` TVIEW among
them. A sweep of a view says its **owned** rows changed: a plain or materialized view owns
everything it reads; a TVIEW owns itself and its own base table (`local` in
`tviews.registry.cascade_kinds`), not the tables it embeds. Sweeping a view also sweeps every
view that reads something it owns. So a user's write reaches `tv_post` (which embeds the
user), and a post's write does not reach `tv_user`. Two names for one relation (`tv_user` and
`public.tv_user`) sweep together.

The graph is read once, when the server starts, like the rest of the cache's schema-derived
state: a TVIEW or view redefined while the server runs is seen at its next start (no
recompile is needed). If the catalog cannot be read, the server logs a warning and a write
evicts only the views it names; cached reads of views embedding them then expire by their TTL
alone. Embedding through a TVIEW is read from `tviews.registry.cascade_kinds`; a `pg_tviews`
whose registry has no such column is read as plain tables (views over them still link).

**Tradeoff**: eviction is per view, not per row. Writing one user evicts every cached read of
`tv_user` and of every view embedding users, including reads that never returned that user.
Over-eviction costs a re-read; under-eviction would serve a stale row for the entry's TTL,
forever at TTL 0.

### A read in flight across a write

A cached read is: look up, miss, query the database, store. A write that commits and evicts
while the query is in flight evicts nothing (the entry is not stored yet), and storing the
rows afterwards would cache what the database held before the write. So the cache refuses to
store a read when, while it was in flight, a write evicted something that entry would have
been evicted by: one of the views it is cached under (its own and its declared secondary
views), its entity type (a write's eviction by id), or a cache clear (#1079).

A write that cannot reach the entry does not refuse it (#1584). A write to `tv_post` (with no
view embedding posts read) does not stop a `tv_user` read from being cached, however many
writes are in flight. Which views a write reaches is decided after the embedding graph above
expands its sweep: a write to `tv_user` refuses an in-flight `tv_post` read.

A write landing during the store itself is caught after the entry is stored: the read
withdraws its own entry.

### Several servers (#1577)

The cache lives in one server process. A write evicts entries **on the server that ran
it**, and nowhere else:

- **One server**: a cached read is never staler than the last write made through it.
- **Several servers** (replicas behind a load balancer, a rolling deploy, a second process
  on the same host): server B keeps serving an entry that a write through server A made
  stale, until the entry's TTL runs out. At `cache_ttl_seconds = 0` it never runs out: B
  serves the stale entry until B restarts, or until a write through B evicts it.
- **Writes FraiseQL does not run** (`psql`, a migration, another application, a cron job)
  evict nothing on any server. Only the TTL bounds them.

So with several servers, a query's `cache_ttl_seconds` is the staleness it accepts after a
write through another server. Give every cached query a TTL above `0` that the data can be
stale for, run the cache on one server only (`cache_enabled = false` on the others), or turn
on the feed below. Without the feed, the server warns at boot naming the queries cached
with `cache_ttl_seconds = 0`.

#### The feed: `cache_cross_instance = "notify"`

```toml
# fraiseql.toml (server)
cache_cross_instance = "notify"   # default "off"
```

Each server then opens one more connection to `database_url`, of its own, and:

- **announces** what each of its writes evicted (the swept views, the evicted entities, an
  operator's `cache/clear`) with `NOTIFY fraiseql_cache`, right after the write commits;
- **applies** every other server's announcements, expanding swept views through its own
  embedding graph, as if the write had run through it.

Measured on one host, another server serves the write's result 0.4 ms after the write
answers when idle, and 3.7 ms (p50) / 20 ms (p99) under 40 concurrent writers; the writers
lose under 2% of their throughput to the feed.

What it does not cover, and what it does about it:

| Case | What happens |
|---|---|
| A write FraiseQL does not run | Nothing is announced; the TTL bounds it, as without the feed. |
| A server crashes between its `COMMIT` and its announcement | That write is never announced; the TTL bounds it. |
| A server's feed connection drops | That server clears its cache and stores nothing until it listens again, then clears it once more. An outage costs hits, never a stale answer. |
| A server starts | It stores nothing until its feed listens. |
| More announcements than the queue holds (16 384), or a failed send | The next message tells every server to clear everything. |
| A message longer than PostgreSQL's 8 000-byte limit | Split into several; a single name too long for any message clears everything. |
| A `LISTEN` that receives nothing (PgBouncer in transaction mode) | The server checks that it hears its own message before it caches; it never does, so it never caches, and logs why. Point `database_url` at PostgreSQL or a session-mode pooler. |

The announcement is sent after commit, not in the write's transaction: PostgreSQL serialises
the commits of every transaction that ran `NOTIFY` on one cluster-wide lock held across the
commit's flush. Measured under 40 concurrent writers on durable storage, notifying inside the
transaction cost 13% of the write throughput and 16% of its median latency.

Metrics: `fraiseql_cache_feed_connected` (0 or 1), `fraiseql_cache_feed_sent_total`,
`fraiseql_cache_feed_received_total`, `fraiseql_cache_feed_reconnects_total`,
`fraiseql_cache_feed_lost_total`.

---

## Operational Notes

**Memory estimation**:

```
cache capacity × average entry size
Example: 10,000 × 10KB = ~100MB
```

**Metrics** exposed on `/metrics`, read from the cache itself (the same counts
`GET /api/v1/admin/cache/stats` reports), and absent when the server runs no result cache:

```
fraiseql_cache_hits_total            counter
fraiseql_cache_misses_total          counter
fraiseql_cache_hit_ratio             gauge (0-1)
fraiseql_cache_entries               gauge
fraiseql_cache_invalidations_total   counter
fraiseql_cache_memory_bytes          gauge (estimated)
```

**Manual invalidation**: invalidation is normally mutation-driven (per-view /
per-entity), and a schema reload clears the cache. For an operator, the admin API
reaches this cache directly:

```bash
# Drop everything
curl -X POST localhost:8000/api/v1/admin/cache/clear \
  -H "Authorization: Bearer $FRAISEQL_ADMIN_TOKEN" -H "Content-Type: application/json" \
  -d '{"scope": "all"}'

# Drop the entries reading one type's view
curl -X POST localhost:8000/api/v1/admin/cache/clear \
  -H "Authorization: Bearer $FRAISEQL_ADMIN_TOKEN" -H "Content-Type: application/json" \
  -d '{"scope": "entity", "entity_type": "Price"}'
```

Both endpoints report each cache separately under `caches[]`, because a server may
also hold the Arrow Flight query-plan cache — a different cache with the same
vocabulary. `scope: "pattern"` applies only to the Arrow cache: result-cache keys are
hashes, not globbable strings, and the response says so rather than reporting a
successful clear of nothing.

**Mutation-invalidated-only caching** for a specific query — set
`cache_ttl_seconds = 0` in the schema (cached, no time-based expiry, evicted by
mutations). It is correct on one server only: another server's mutations never evict it
([Several servers](#several-servers-1577)).

```python
@fraiseql.query(sql_source="v_live_prices", cache_ttl_seconds=0)
def live_prices() -> list[Price]:
    ...
```
