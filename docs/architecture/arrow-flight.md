# Arrow Flight

The `arrow_flight` capability serves query results as Apache Arrow record batches over
the [Arrow Flight](https://arrow.apache.org/docs/format/Flight.html) gRPC protocol, for
clients that want columns rather than JSON: dataframes, notebooks, warehouses. It also
accepts uploads into tables you allow-list.

It is behind the `arrow` Cargo feature, which the `-full` and `-platform` release
artifacts compile in, and runs on PostgreSQL. The `fraiseql-server` binary and
`fraiseql run` start it. `serve_on_listener` does not: an embedder that hands the server
its listener gets no port it did not choose, so it gets no Flight listener either.

## Configuration

```toml
# server.toml
flight_bind_addr = "127.0.0.1:50051"   # the default: loopback only
flight_views = ["va_public_metrics"]   # the default is none
flight_upload_tables = []              # the default: Upload disabled

[auth]
issuer = "https://id.example.com/"     # Flight trusts the server's OIDC validator
audience = "analytics"
```

| Key | Default | Meaning |
|-----|---------|---------|
| `flight_bind_addr` | `127.0.0.1:50051` | Address of the Flight listener. Binding another interface is an explicit choice. `--flight-bind-addr` and `FRAISEQL_FLIGHT_BIND_ADDR` override it; a malformed value stops startup. |
| `flight_views` | `[]` | Views an `OptimizedView` ticket may read. Each must be a plain identifier, or boot fails. Each is typed from one of its rows at boot, so a view that is empty or unreadable then is not served (a warning says so). |
| `flight_upload_tables` | `[]` | Tables a client may `DoPut` into. Empty disables uploads. |

The handshake also needs `FLIGHT_SESSION_SECRET` in the environment, generated with
`openssl rand -hex 32`. It signs the short-lived session token a client uses after the
handshake. Without it, the server starts, logs a warning, and fails every handshake.

### What a view and an upload bypass

Read this before naming anything in either list:

- **`flight_views`**: an `OptimizedView` ticket reads the view **whole**, with no row
  policy, field rule or authorizer
  ([#716](https://github.com/fraiseql/fraiseql/issues/716)). Every principal that
  completes the handshake can read every row and column of every view named here. Name
  only views whose entire contents all of them may read.
- **`flight_upload_tables`**: an upload is a client-directed `INSERT` that does not go
  through the mutation pipeline, so no operation authorizer or field rule applies.
  - Name only tables every authenticated Flight client may write.
  - The rows and their `core.tb_entity_change_log` entries are written in one
    transaction, so subscribers and CDC see an upload like any other write.
  - The server refuses to boot with uploads enabled alongside `cache_enabled = true`:
    an upload invalidates no cached read.
  - It also refuses uploads alongside `[tenancy.runtime] enabled`: an upload writes
    through the server-wide pool, not a tenant's
    ([#1550](https://github.com/fraiseql/fraiseql/issues/1550)).

## Authentication

1. The client sends `Handshake` with the payload `Bearer <jwt>`. The token is checked by
   the server's OIDC validator, built from `[auth]`: issuer, audience and the issuer's
   published keys. The HS256 bearer mode of the HTTP transports does not apply to
   Flight.
2. The handshake answers with a session token, signed with `FLIGHT_SESSION_SECRET` and
   valid for five minutes. Every later call carries it as
   `authorization: Bearer <session token>`.

With no `[auth]`, the server logs that Flight runs without authentication, and the
handshake still refuses every client: there is no unauthenticated Flight access.
Identity enrichment applies to Flight principals as it does to HTTP ones.

## Calls and tickets

A ticket is a JSON object tagged by `type`:

| Ticket | Serves |
|--------|--------|
| `{"type":"GraphQLQuery","query":"…","variables":…}` | A GraphQL query's result, executed through the same policy as `/graphql`: tenant dispatch, trusted documents, per-tenant quotas |
| `{"type":"OptimizedView","view":"…"}` | A view named in `flight_views` (above) |
| `{"type":"BatchedQueries",…}`, `{"type":"BulkExport",…}`, `{"type":"ObserverEvents",…}` | Several queries, a table export, and observer events |

- **`DoGet`** streams a ticket's batches. By default 50 streams run at once; the next
  one answers `RESOURCE_EXHAUSTED`.
- **`GetFlightInfo`** and **`GetSchema`** take the ticket in the descriptor's path.
  `PollFlightInfo` answers at once, complete.
- **`DoPut`** uploads into an allow-listed table. Any other table answers
  `PERMISSION_DENIED`.
- **`DoExchange`** carries queries and uploads.
- **`DoAction`** offers `HealthCheck`, plus `ClearCache` and `RefreshSchemaRegistry`,
  which need the `admin` scope.
- **`ListFlights`** does not list the declared `flight_views` today
  ([#1789](https://github.com/fraiseql/fraiseql/issues/1789)). A client has to know the
  view's name.

GraphQL types map to Arrow types:

- `ID`, `UUID` and `JSON` map to `Utf8`.
- `DateTime` maps to `Timestamp(ns, UTC)`.
- `Date` maps to `Date32`.
- `Time` maps to `Time64`.
- `Decimal` maps to `Decimal128(38, 10)`.
- Any other type maps to `Utf8`.

Rows read straight from the database are a separate case. Today their numeric columns
are rounded through `Float64`, which loses the scale and any digit past the
seventeenth. This affects view tickets and fact-table measures
([#1766](https://github.com/fraiseql/fraiseql/issues/1766)).

## See also

- [Feature compatibility matrix](../features-compatibility-matrix.md)
- [Enriched identity](enriched-identity-rls.md)
- `crates/fraiseql-arrow/README.md`: the library, for embedders
- The `arrow_flight` capability fixture,
  `crates/fraiseql-server/tests/capabilities/arrow_flight/`. Through the server binary,
  an OIDC-authenticated client completes the handshake and reads a declared view with
  `DoGet`.
