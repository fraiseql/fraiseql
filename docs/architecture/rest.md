# REST Transport

The `rest` capability serves the compiled schema's queries and mutations as REST
resources: no GraphQL document is parsed, and every request runs on the same executor,
with the same authentication, row policies and field rules, as a GraphQL request.

It is behind the `rest` Cargo feature, which the `-full` and `-platform` release
artifacts compile in, and is off until the schema turns it on.

## Turning it on

```toml
# fraiseql.toml
[rest]
enabled = true
path = "/rest/v1"        # the default
```

`fraiseql compile` writes the section into the compiled schema as `rest_config`; the
server mounts the routes when `rest_config.enabled` is true. Unknown keys in `[rest]`
are refused at compile time, and so is the retired `max_embedded_reads`, whose error
names the key that replaced it.

| Key | Default | Meaning |
|-----|---------|---------|
| `path` | `"/rest/v1"` | Mount point of every resource and of `openapi.json` |
| `require_auth` | `false` | `true` answers `401 UNAUTHENTICATED` on every route, `openapi.json` included, without a bearer |
| `default_page_size` / `max_page_size` | `100` / `1000` | `limit` and `first` are clamped to the maximum |
| `default_embed_page_size` | `50` | Rows per embedded relation when `?rel.limit=` is not given |
| `max_embedding_depth` | `3` | Deepest `?select=` nesting accepted |
| `max_filter_bytes` | `4096` | Longest `?filter=` accepted |
| `max_bulk_affected` | `10000` | Rows a collection-level `PATCH` or `DELETE` may touch |
| `ndjson_batch_size` | `500` | Rows fetched per batch for an NDJSON stream |
| `delete_response` | `"no_content"` | `"entity"` answers a `DELETE` with `200` and the deleted row |
| `etag` | `true` | `ETag` on reads; `If-None-Match` answers `304` |
| `idempotency_ttl_seconds` | `300` | How long an `Idempotency-Key` is remembered |
| `default_cache_ttl` / `cdn_max_age` | `0` / unset | Response cache lifetime and `Cache-Control: s-maxage` |
| `sse_heartbeat_seconds` / `sse_max_replay_events` | `30` / `10000` | Server-sent event streams (below) |
| `include` / `exclude` | `[]` | Operations exposed as resources |

## Routes

Each query and mutation becomes a route under `path`. An operation can name its own
path and method in the authoring SDK; otherwise:

| Operation | Route |
|-----------|-------|
| List query | `GET /{resource}` |
| Single-item query | `GET /{resource}/{id}` |
| Insert mutation | `POST /{resource}` → `201` with `Location` |
| Full update | `PUT` and `PATCH /{resource}/{id}` |
| Partial update | `PATCH /{resource}/{id}/{action}` |
| Delete | `DELETE /{resource}/{id}` → `204` |
| Any other mutation | `POST /{resource}/{id}/{action}` or `POST /{resource}/{action}` |

The id argument is the operation's `id` argument, or its `pk_*` argument when it has
none. Collection-level `PATCH` and `DELETE` apply one change to every row a filter
matches, up to `max_bulk_affected`. `GET {path}/openapi.json` serves an OpenAPI 3.0.3
document of the mounted routes; it advertises bearer security only when the server
has an `[auth]` validator to enforce it.

The server binary mounts the write routes. An embedder that builds a `Server` itself
mounts reads and server-sent events only, unless it calls
`Server::with_rest_write_surface`; over a read-only executor those write routes mount
and refuse every request.

## Requests

- **Filters:** `?name=x` is equality; `?price[gte]=10` takes an operator: `eq`, `ne`,
  `gt`, `gte`, `lt`, `lte`, `in`, `nin`, `like`, `ilike`, `contains`, `icontains`,
  `startswith`, `istartswith`, `endswith`, `iendswith`, `is_null`, `is_not_null`.
  `?filter=` takes a JSON filter, and `or`, `and` and `not` combine them.
- **Selection and embedding:** `?select=id,title,author(name)` picks fields and embeds
  relations. An embed is read in the same SQL statement as its parent; its total cost
  counts against `[security.cost_budget] per_request_max` (see
  [GraphQL complexity limits](../security/graphql-complexity-limits.md)).
- **Pagination:** `limit`/`offset`, or `first`/`after` and `last`/`before` cursors.
- **Ordering and search:** `?sort=` and `?search=`.
- **`Prefer`:** `count=`, `return=`, `resolution=`, `tx=`, `handling=`,
  `max-affected=`.

## Responses

A single item is `{"data": …}`; a collection is `{"data": […], "meta": …, "links": …}`,
with `meta` carrying `limit`/`offset` or `hasNextPage`/`hasPreviousPage`. Errors are
`{"error": {"code", "message", "details"?}}` with the matching status: `400`
`BAD_REQUEST`, `BAD_USER_INPUT` or `CONSTRAINT_VIOLATION`, `401 UNAUTHENTICATED`, `403
FORBIDDEN`, `404 NOT_FOUND`, `406 NOT_ACCEPTABLE`, `409 CONFLICT`, `413
RESPONSE_TOO_LARGE`, `422 UNPROCESSABLE_ENTITY`, `429 RATE_LIMITED`, `503
SERVICE_UNAVAILABLE`, `500 INTERNAL_SERVER_ERROR`. Database text never reaches the
client.

### Streams and exports

- `Accept: application/x-ndjson` streams a collection as one JSON object per line, with
  no envelope, `ndjson_batch_size` rows at a time.
- `GET /{resource}/stream` is a server-sent event stream of the resource's changes (it
  needs the `observers` feature). A client that reconnects with `Last-Event-ID` resumes
  where it left off: see [REST stream resumption](../operations/rest-stream-resumption.md).
- With the `export-csv` and `export-xlsx` features, `Accept: text/csv` and the XLSX
  media type export a collection. The server's `[export]` table sets the CSV delimiter
  and byte-order mark, the row limits and the formats offered:

```toml
# server.toml
[export]
export_formats = ["csv", "xlsx"]   # an empty list turns exports off
xlsx_max_rows = 100000
max_concurrent_xlsx = 10           # past it: 503 XLSX_BUSY with Retry-After
```

A format left out of `export_formats` answers `406 EXPORT_FORMAT_DISABLED`. Exports and
writes are sent `Cache-Control: no-store` (see [caching](../modules/cache.md)).

## See also

- [Feature compatibility matrix](../features-compatibility-matrix.md)
- [Authoring](../authoring.md): the `relationships` a type declares are what `?select=`
  can embed
- The `rest` capability fixture, `crates/fraiseql-server/tests/capabilities/rest/`:
  a `POST` that inserts a row, through the server binary.
