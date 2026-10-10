# GraphQL Complexity Limits

Audit of all GraphQL query complexity and abuse protections in FraiseQL.

## Implemented Protections

| Limit | Default | Configurable | Location |
|-------|---------|-------------|----------|
| **Alias amplification** | 30 aliases | No (hardcoded) | `crates/fraiseql-server/src/validation.rs:459` |
| **Query depth** | 10 levels | Yes (`max_query_depth` in `fraiseql.toml`) | `crates/fraiseql-server/src/validation.rs:457` |
| **Top-level page size** | 1000 rows | Yes (`max_page_size` in `[validation]`, `FRAISEQL_MAX_PAGE_SIZE` env) | `crates/fraiseql-core/src/runtime/executor/runners/query_params.rs` (`enforce_max_page_size`) |
| **Offset depth** | none | Yes (`max_offset` in `[validation]`, `FRAISEQL_MAX_OFFSET` env) | `crates/fraiseql-core/src/runtime/executor/runners/query_params.rs` (`enforce_max_offset`); see [pagination](../features/pagination.md#deep-offset) |
| **Complexity error rate** | 30 errors/60s per key | Yes (`complexity_errors_max_requests`) | `crates/fraiseql-core/src/validation/rate_limiting.rs:67` |
| **Federation batch** | 1000 representations | No (hardcoded) | `crates/fraiseql-server/src/federation/` |

### Alias Amplification (Hardcoded at 30)

```
Location: crates/fraiseql-server/src/validation.rs
  Line 459: max_aliases_per_query: 30 (default in RequestValidator)
  Line 179: if alias_count > self.max_aliases_per_query → 429 TooManyRequests
```

A query with 31+ aliases on the same field is rejected. This prevents a client from
forcing the server to resolve the same field 1000+ times via aliasing.

### Query Depth (Default 11, Configurable)

```
Default: fraiseql_core::schema::DEFAULT_MAX_QUERY_DEPTH = 11

fraiseql.toml override:
  [validation]
  max_query_depth = 15
```

A query exceeding the depth limit is rejected with a `QueryTooDeep` error before reaching
the database. The same bound decides how deep the selection resolver and the projectors
follow a document, so every level a query may reach is projected through its selection.
11 is the depth GraphQL queries were held to before there was a default; past about 49 a
declared value cannot bind on GraphQL, whose parser refuses a document nested past 50
brackets.

A REST leaf `?select=` of an object field (`members?select=id,team`) means the whole
object: every field its type declares, each gated as a read of that type. That expansion
follows the schema, not a selection, so it is bounded on its own: it goes at most **4 object
levels** down. A type that reaches itself (`Folder.parent`), or any object nested deeper,
answers **400** rather than being truncated or served as stored. To read deeper, name the
fields you want through GraphQL, whose sub-selections are bounded by `max_query_depth`.

### Top-Level Page Size (Default 1000, Configurable)

The `first`/`last`/`limit` argument on a **root** query is capped before it reaches SQL
(#421). This is the one pagination knob that sizes the database scan, the materialized
JSONB, and the response buffer — an arbitrarily large value is an unbounded-pagination
denial-of-service lever. (Nested `first`/`limit` is an in-memory slice of an already-fetched
JSONB array in FraiseQL's view model, so it is not a separate cap.)

```
Location: crates/fraiseql-core/src/runtime/executor/runners/query_params.rs (enforce_max_page_size)
  Enforced in the regular runner (query_regular.rs) and the relay runner (query_relay.rs).

Precedence (highest first):
  FRAISEQL_MAX_PAGE_SIZE env  (a number, or 0/none to disable)
  [validation] max_page_size  (fraiseql.toml → compiled schema)
  RuntimeConfig::max_page_size default (1000)
```

A request exceeding the ceiling is rejected with a `Validation` error
(`` `first` 5000000 exceeds the maximum page size of 1000 ``) before any SQL is issued.

### Complexity Error Rate Limiting

When queries fail complexity validation (depth/alias), the error itself is rate-limited
to prevent probing attacks:

```
Location: crates/fraiseql-core/src/validation/rate_limiting.rs
  Line 67: complexity_errors_max_requests: 30 (per 60-second window)
```

---

## Cost Budgets (#379)

The estimated operation cost (the complexity score, with root-operation
`[fraiseql.cost_weights]` overriding their subtree walk) is enforced at three
levels:

| Level | Config | Rejection | Scope |
|-------|--------|-----------|-------|
| Schema-wide per-request ceiling | `[security.cost_budget] per_request_max` | `OPERATION_COST_EXCEEDED` (200 + `errors[]`, not retryable); `BAD_REQUEST` 400 over REST; `RESOURCE_EXHAUSTED` over gRPC | **Inside the executor** — every transport that executes a GraphQL document (`/graphql` POST/GET/QUERY, MCP, the functions bridge, direct embedders), **and** every direct read that never had a document (the REST read surface, both gRPC read arms) |
| Per-tenant per-request budget | tenant-quota admin API `cost_budget` | `OPERATION_COST_EXCEEDED` | `/graphql`, at the shared tenant-dispatch seam |
| Per-tenant rolling minute window | tenant-quota admin API `cost_budget_per_minute`, defaulted by `[security.cost_budget] per_tenant_per_minute_default` | `COST_BUDGET_EXHAUSTED` (429 + `Retry-After`) | `/graphql`, same seam |

It is a ceiling on **one request**, not on one read. A GraphQL document states
its whole shape and is scored whole. A REST `?select=` embed is scored whole too:
its embedded levels are composed into the parent's SQL statement (one correlated
`LATERAL` subquery per level, each with its own page), and the statement is
scored as the tree it is — every level's fields multiplied by every page above
it — before it is sent. An embedded `rel.count` is part of that statement and
adds one per parent row.

The score is a **bound**: each level is charged its full page, whatever rows
exist. An embedded level's page is `?rel.limit=` when the client names one
(`?orders.items.limit=` for a nested level; refused above `[rest] max_page_size`),
and `[rest] default_embed_page_size` (50) otherwise.
`users?select=id,orders(id,total)` with the default parent page of 100 is charged
`1 + 100 × (1 + 1 + 2 × 50)` = 10 201 on a table of two users. Pass `?limit=` and
`?orders.limit=` to be charged for the pages you read.

The `Prefer: count=exact` total is the exception, and a known gap: it is
answered through a second read chokepoint that carries no cost gate at all.

Declared `[validation]` depth/complexity limits likewise bind **inside the
executor** (derived at construction), so a bound declared in the compiled
schema holds on every transport, not only at the HTTP stage. Transports that
never execute client-authored documents are bounded structurally: MCP builds
its documents from the schema (constant cost per tool — the per-tenant budget
deliberately does not meter it), subscriptions honor only the root field name,
and the Flight service refuses ad-hoc GraphQL outright (no executor attached).

Observability: every `/graphql` request logs its estimated cost with tenant
and operation on the `fraiseql::cost_audit` tracing target, and the running
sum is exported as `fraiseql_graphql_queries_cost_total` — size budgets from
observed traffic before enforcing them. Registered persisted documents can be
costed ahead of deployment with
`fraiseql validate-documents manifest.json --max-cost N [--schema schema.compiled.json]`.

---

## Not Implemented

| Attack | Status | Notes |
|--------|--------|-------|
| **Per-field cost weights** | ⚠️ Partial (#379) | Cost budgets, root-operation `@cost` weights, per-tenant windows and the schema-wide ceiling are enforced (see above). Fine-grained *per-field* cost scoring is not present — non-weighted roots score the type-agnostic complexity count. |
| **Fragment cycle detection** | ❓ Unverified | `graphql-parser` crate handles AST parsing; cycle detection depends on the library. |
| **Introspection disable** | ❓ Unverified | No `disable_introspection` flag found in `validation.rs`. Check `routes/graphql/handler.rs`. |
| **Batch query amplification** | ❓ Unverified | HTTP batching (array of operations) not confirmed present or absent. |
| **Field count explosion** | ❌ Not implemented | No `max_fields_per_query` limit. |

---

## Configuration

```toml
# fraiseql.toml
[validation]
# Max query nesting depth (default: 11)
max_query_depth = 11
# Max rows a top-level first/last/limit may request (default: 1000). #421
# Overridable at runtime with FRAISEQL_MAX_PAGE_SIZE (a number, or 0/none to disable).
max_page_size = 1000
# Deepest offset a client may page to (default: none). #1306
# Overridable at runtime with FRAISEQL_MAX_OFFSET (a number, or 0/none to lift it).
# max_offset = 10000
# Most bytes one read may deliver (default: none). Charged on the rows that come back,
# inside the executor, so it binds on every transport that returns rows. Refused as
# 413 PAYLOAD_TOO_LARGE (GraphQL, SSE, MCP, async operations, REST) and
# RESOURCE_EXHAUSTED (gRPC, both arms). #1351, #1543
# The server's runtime config may override it under its own [validation]; that value
# holds across a hot reload (SIGUSR1) and binds every registered tenant. #1534
# max_response_bytes = 10485760

[security.cost_budget]
# Hard per-operation cost ceiling, enforced inside the executor for every
# transport (#379). Omit for no ceiling.
per_request_max = 10000
# Default rolling per-minute budget for registered tenants without their own
# cost_budget_per_minute. Omit for no default.
per_tenant_per_minute_default = 100000

[fraiseql.security]
# Per-operation @cost weights are declared under [fraiseql.cost_weights]; per-tenant
# cost budgets are configured via the tenant-quota admin API (cost_budget,
# cost_budget_per_minute). (#379)

[fraiseql.security.rate_limiting]
# Rate limit for complexity error responses (default: 30 per 60s)
complexity_errors_max_requests = 30
complexity_errors_window_secs  = 60
```

---

## Recommended Gaps to Address

1. **Fragment cycle detection** — confirm `graphql-parser` handles this or add explicit check
2. **Introspection control** — add `allow_introspection: bool` flag to `RequestValidator`
3. **Per-field cost scoring** — extend the type-agnostic complexity score with per-field weights (per-tenant `cost_budget` and root-operation `@cost` weights already shipped in #379)
4. **Field count limit** — add `max_fields_per_selection_set: usize` to `RequestValidator`
